//! `buffa::Message` implementations for the mediatime types, behind the
//! `buffa` feature. Used via `extern_path` from buffa-generated crates.
//!
//! Wire format (clean redesign — no compatibility with findit-proto's
//! hand-rolled encoding is required):
//!   Timebase  { int32  num = 1;  int32  den = 2; }
//!   TimeRange { int64  start = 1; int64  end = 2; Timebase timebase = 3; }
//!   Timestamp { int64  pts = 1;  Timebase timebase = 2; }
//!
//! The nested `Timebase` is always encoded (presence-independent) so that
//! `decode(encode(x)) == x` holds unconditionally.
//!
//! `Timebase`'s fields were `uint32` before the type became signed. Protobuf's
//! `int32` and `uint32` are the same plain (non-ZigZag) varint for values a
//! `Timebase` can hold — both are non-negative and at most `i32::MAX` — so the
//! bytes are unchanged in both directions. `sint32` would have been the
//! silent break: ZigZag re-encodes every value.

use core::num::NonZeroI32;

use ::buffa::{
  DecodeContext, DecodeError, DefaultInstance, EncodeSink, Message, SizeCache,
  bytes::Buf,
  encoding::{Tag, WireType, encode_varint, skip_field_depth, varint_len},
  types::{
    decode_int32, decode_int64, encode_int32, encode_int64, int32_encoded_len, int64_encoded_len,
  },
};

use crate::{DEN_ONE, TimeRange, Timebase, Timestamp};

const VARINT: u8 = WireType::Varint as u8;
const LEN: u8 = WireType::LengthDelimited as u8;

// ----------------------------------------------------------------------------
// Timebase — leaf message { int32 num = 1; int32 den = 2; }
// ----------------------------------------------------------------------------

impl DefaultInstance for Timebase {
  fn default_instance() -> &'static Self {
    static VALUE: buffa::__private::OnceBox<Timebase> = buffa::__private::OnceBox::new();
    VALUE.get_or_init(|| buffa::alloc::boxed::Box::new(Timebase::default()))
  }
}

impl Message for Timebase {
  // `num`/`den` are encoded UNCONDITIONALLY — no proto3 "skip default (0)"
  // elision. buffa's decoder seeds the message from `Timebase::default()`
  // (mediatime's Default = 1/1), NOT proto3 zero. Eliding `num == 0` would
  // therefore decode back as `num == 1` and break round-trip for e.g.
  // `Timebase::new(0, _)`. Both tags are single-byte (fields 1 and 2 < 16).
  fn compute_size(&self, _cache: &mut SizeCache) -> u32 {
    2 + int32_encoded_len(self.num()) as u32 + int32_encoded_len(self.den().get()) as u32
  }

  fn write_to(&self, _cache: &mut SizeCache, buf: &mut impl EncodeSink) {
    Tag::new(1, WireType::Varint).encode(buf);
    encode_int32(self.num(), buf);
    Tag::new(2, WireType::Varint).encode(buf);
    encode_int32(self.den().get(), buf);
  }

  fn merge_field(
    &mut self,
    tag: Tag,
    buf: &mut impl Buf,
    ctx: DecodeContext<'_>,
  ) -> Result<(), DecodeError> {
    match tag.field_number() {
      1 => {
        if tag.wire_type() != WireType::Varint {
          return Err(DecodeError::WireTypeMismatch {
            field_number: 1,
            expected: VARINT,
            actual: tag.wire_type() as u8,
          });
        }
        // `Timebase::new` panics on a negative numerator, so decode must not
        // hand it one. Our own encoder never writes a negative; a peer can,
        // either directly or by writing a `uint32` above `i32::MAX` that
        // `decode_int32` truncates into the negative half. Clamp to the
        // smallest legal numerator so decode stays total, as `den` does.
        let num = decode_int32(buf)?.max(0);
        *self = Timebase::new(num, self.den());
      }
      2 => {
        if tag.wire_type() != WireType::Varint {
          return Err(DecodeError::WireTypeMismatch {
            field_number: 2,
            expected: VARINT,
            actual: tag.wire_type() as u8,
          });
        }
        // A malformed den — zero, or negative by the same route as `num`
        // above — is clamped to 1 to keep decode total. `NonZeroI32::MIN` is
        // `i32::MIN`, so the clamp target is spelled out as `DEN_ONE`.
        let den = NonZeroI32::new(decode_int32(buf)?)
          .filter(|d| d.get() > 0)
          .unwrap_or(DEN_ONE);
        *self = Timebase::new(self.num(), den);
      }
      _ => skip_field_depth(tag, buf, ctx.depth())?,
    }
    Ok(())
  }

  fn clear(&mut self) {
    *self = Timebase::default();
  }
}

// ----------------------------------------------------------------------------
// TimeRange — { int64 start = 1; int64 end = 2; Timebase timebase = 3; }
// ----------------------------------------------------------------------------

impl DefaultInstance for TimeRange {
  fn default_instance() -> &'static Self {
    static VALUE: buffa::__private::OnceBox<TimeRange> = buffa::__private::OnceBox::new();
    VALUE.get_or_init(|| buffa::alloc::boxed::Box::new(TimeRange::new(0, 0, Timebase::default())))
  }
}

impl Message for TimeRange {
  fn compute_size(&self, cache: &mut SizeCache) -> u32 {
    let mut size = 0u32;
    // proto3 zero-elision: sound here — the decoder seeds start/end/pts at 0.
    if self.start_pts() != 0 {
      size += 1 + int64_encoded_len(self.start_pts()) as u32;
    }
    // proto3 zero-elision: sound here — the decoder seeds start/end/pts at 0.
    if self.end_pts() != 0 {
      size += 1 + int64_encoded_len(self.end_pts()) as u32;
    }
    // timebase (field 3) — always encoded for unconditional round-trip.
    let slot = cache.reserve();
    let inner = self.timebase().compute_size(cache);
    cache.set(slot, inner);
    size += 1 + varint_len(inner as u64) as u32 + inner;
    size
  }

  fn write_to(&self, cache: &mut SizeCache, buf: &mut impl EncodeSink) {
    // proto3 zero-elision: sound here — the decoder seeds start/end/pts at 0.
    if self.start_pts() != 0 {
      Tag::new(1, WireType::Varint).encode(buf);
      encode_int64(self.start_pts(), buf);
    }
    // proto3 zero-elision: sound here — the decoder seeds start/end/pts at 0.
    if self.end_pts() != 0 {
      Tag::new(2, WireType::Varint).encode(buf);
      encode_int64(self.end_pts(), buf);
    }
    Tag::new(3, WireType::LengthDelimited).encode(buf);
    encode_varint(cache.consume_next() as u64, buf);
    self.timebase().write_to(cache, buf);
  }

  fn merge_field(
    &mut self,
    tag: Tag,
    buf: &mut impl Buf,
    ctx: DecodeContext<'_>,
  ) -> Result<(), DecodeError> {
    match tag.field_number() {
      1 => {
        if tag.wire_type() != WireType::Varint {
          return Err(DecodeError::WireTypeMismatch {
            field_number: 1,
            expected: VARINT,
            actual: tag.wire_type() as u8,
          });
        }
        let v = decode_int64(buf)?;
        // Use the bypass constructor: intermediate state may have
        // start > end if `start` field arrives before `end`.
        *self = TimeRange::new_for_decode(v, self.end_pts(), self.timebase());
      }
      2 => {
        if tag.wire_type() != WireType::Varint {
          return Err(DecodeError::WireTypeMismatch {
            field_number: 2,
            expected: VARINT,
            actual: tag.wire_type() as u8,
          });
        }
        let v = decode_int64(buf)?;
        // Use the bypass constructor: intermediate state may have
        // start > end if `end` field arrives before `start`.
        *self = TimeRange::new_for_decode(self.start_pts(), v, self.timebase());
      }
      3 => {
        if tag.wire_type() != WireType::LengthDelimited {
          return Err(DecodeError::WireTypeMismatch {
            field_number: 3,
            expected: LEN,
            actual: tag.wire_type() as u8,
          });
        }
        let mut tb = self.timebase();
        buffa::Message::merge_length_delimited(&mut tb, buf, ctx)?;
        *self = TimeRange::new_for_decode(self.start_pts(), self.end_pts(), tb);
      }
      _ => skip_field_depth(tag, buf, ctx.depth())?,
    }
    Ok(())
  }

  fn clear(&mut self) {
    *self = TimeRange::new(0, 0, Timebase::default());
  }
}

// ----------------------------------------------------------------------------
// Timestamp — { int64 pts = 1; Timebase timebase = 2; }
// ----------------------------------------------------------------------------

impl DefaultInstance for Timestamp {
  fn default_instance() -> &'static Self {
    static VALUE: buffa::__private::OnceBox<Timestamp> = buffa::__private::OnceBox::new();
    VALUE.get_or_init(|| buffa::alloc::boxed::Box::new(Timestamp::new(0, Timebase::default())))
  }
}

impl Message for Timestamp {
  fn compute_size(&self, cache: &mut SizeCache) -> u32 {
    let mut size = 0u32;
    // proto3 zero-elision: sound here — the decoder seeds start/end/pts at 0.
    if self.pts() != 0 {
      size += 1 + int64_encoded_len(self.pts()) as u32;
    }
    let slot = cache.reserve();
    let inner = self.timebase().compute_size(cache);
    cache.set(slot, inner);
    size += 1 + varint_len(inner as u64) as u32 + inner;
    size
  }

  fn write_to(&self, cache: &mut SizeCache, buf: &mut impl EncodeSink) {
    // proto3 zero-elision: sound here — the decoder seeds start/end/pts at 0.
    if self.pts() != 0 {
      Tag::new(1, WireType::Varint).encode(buf);
      encode_int64(self.pts(), buf);
    }
    Tag::new(2, WireType::LengthDelimited).encode(buf);
    encode_varint(cache.consume_next() as u64, buf);
    self.timebase().write_to(cache, buf);
  }

  fn merge_field(
    &mut self,
    tag: Tag,
    buf: &mut impl Buf,
    ctx: DecodeContext<'_>,
  ) -> Result<(), DecodeError> {
    match tag.field_number() {
      1 => {
        if tag.wire_type() != WireType::Varint {
          return Err(DecodeError::WireTypeMismatch {
            field_number: 1,
            expected: VARINT,
            actual: tag.wire_type() as u8,
          });
        }
        let v = decode_int64(buf)?;
        *self = Timestamp::new(v, self.timebase());
      }
      2 => {
        if tag.wire_type() != WireType::LengthDelimited {
          return Err(DecodeError::WireTypeMismatch {
            field_number: 2,
            expected: LEN,
            actual: tag.wire_type() as u8,
          });
        }
        let mut tb = self.timebase();
        buffa::Message::merge_length_delimited(&mut tb, buf, ctx)?;
        *self = Timestamp::new(self.pts(), tb);
      }
      _ => skip_field_depth(tag, buf, ctx.depth())?,
    }
    Ok(())
  }

  fn clear(&mut self) {
    *self = Timestamp::new(0, Timebase::default());
  }
}

#[cfg(test)]
mod tests;
