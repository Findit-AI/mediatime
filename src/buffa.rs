//! `buffa::Message` implementations for the mediatime types, behind the
//! `buffa` feature. Used via `extern_path` from buffa-generated crates.
//!
//! Wire format (clean redesign — no compatibility with findit-proto's
//! hand-rolled encoding is required):
//!   Timebase  { uint32 num = 1;  uint32 den = 2; }
//!   TimeRange { int64  start = 1; int64  end = 2; Timebase timebase = 3; }
//!   Timestamp { int64  pts = 1;  Timebase timebase = 2; }
//!
//! The nested `Timebase` is always encoded (presence-independent) so that
//! `decode(encode(x)) == x` holds unconditionally.

use core::num::NonZeroU32;

use ::buffa::{
  DecodeError, DefaultInstance, Message, SizeCache,
  bytes::{Buf, BufMut},
  encoding::{Tag, WireType, encode_varint, skip_field_depth, varint_len},
  types::{
    decode_int64, decode_uint32, encode_int64, encode_uint32, int64_encoded_len, uint32_encoded_len,
  },
};

use crate::{TimeRange, Timebase, Timestamp};

const VARINT: u8 = WireType::Varint as u8;
const LEN: u8 = WireType::LengthDelimited as u8;

// ----------------------------------------------------------------------------
// Timebase — leaf message { uint32 num = 1; uint32 den = 2; }
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
    2 + uint32_encoded_len(self.num()) as u32 + uint32_encoded_len(self.den().get()) as u32
  }

  fn write_to(&self, _cache: &mut SizeCache, buf: &mut impl BufMut) {
    Tag::new(1, WireType::Varint).encode(buf);
    encode_uint32(self.num(), buf);
    Tag::new(2, WireType::Varint).encode(buf);
    encode_uint32(self.den().get(), buf);
  }

  fn merge_field(&mut self, tag: Tag, buf: &mut impl Buf, depth: u32) -> Result<(), DecodeError> {
    match tag.field_number() {
      1 => {
        if tag.wire_type() != WireType::Varint {
          return Err(DecodeError::WireTypeMismatch {
            field_number: 1,
            expected: VARINT,
            actual: tag.wire_type() as u8,
          });
        }
        let num = decode_uint32(buf)?;
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
        // den is NonZeroU32; a malformed 0 on the wire (never produced
        // by our own encoder) is clamped to 1 to keep decode total.
        let den = NonZeroU32::new(decode_uint32(buf)?).unwrap_or(NonZeroU32::MIN);
        *self = Timebase::new(self.num(), den);
      }
      _ => skip_field_depth(tag, buf, depth)?,
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

  fn write_to(&self, cache: &mut SizeCache, buf: &mut impl BufMut) {
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

  fn merge_field(&mut self, tag: Tag, buf: &mut impl Buf, depth: u32) -> Result<(), DecodeError> {
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
        buffa::Message::merge_length_delimited(&mut tb, buf, depth)?;
        *self = TimeRange::new_for_decode(self.start_pts(), self.end_pts(), tb);
      }
      _ => skip_field_depth(tag, buf, depth)?,
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

  fn write_to(&self, cache: &mut SizeCache, buf: &mut impl BufMut) {
    // proto3 zero-elision: sound here — the decoder seeds start/end/pts at 0.
    if self.pts() != 0 {
      Tag::new(1, WireType::Varint).encode(buf);
      encode_int64(self.pts(), buf);
    }
    Tag::new(2, WireType::LengthDelimited).encode(buf);
    encode_varint(cache.consume_next() as u64, buf);
    self.timebase().write_to(cache, buf);
  }

  fn merge_field(&mut self, tag: Tag, buf: &mut impl Buf, depth: u32) -> Result<(), DecodeError> {
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
        buffa::Message::merge_length_delimited(&mut tb, buf, depth)?;
        *self = Timestamp::new(self.pts(), tb);
      }
      _ => skip_field_depth(tag, buf, depth)?,
    }
    Ok(())
  }

  fn clear(&mut self) {
    *self = Timestamp::new(0, Timebase::default());
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn nz(n: u32) -> NonZeroU32 {
    NonZeroU32::new(n).unwrap()
  }

  // ---- Timebase ----

  #[test]
  fn timebase_default_instance_and_clear() {
    assert_eq!(
      *<Timebase as DefaultInstance>::default_instance(),
      Timebase::default()
    );
    let mut tb = Timebase::new(7, nz(9));
    Message::clear(&mut tb);
    assert_eq!(tb, Timebase::default());
  }

  #[test]
  fn timebase_field1_wrong_wire_type_errors() {
    let mut buf: Vec<u8> = Vec::new();
    Tag::new(1, WireType::LengthDelimited).encode(&mut buf);
    encode_varint(0, &mut buf);
    let err = <Timebase as Message>::decode_from_slice(&buf).unwrap_err();
    assert!(
      matches!(err, DecodeError::WireTypeMismatch { field_number: 1, expected, actual }
        if expected == VARINT && actual == LEN),
      "got {err:?}"
    );
  }

  #[test]
  fn timebase_field2_wrong_wire_type_errors() {
    let mut buf: Vec<u8> = Vec::new();
    Tag::new(1, WireType::Varint).encode(&mut buf);
    encode_uint32(5, &mut buf);
    Tag::new(2, WireType::LengthDelimited).encode(&mut buf);
    encode_varint(0, &mut buf);
    let err = <Timebase as Message>::decode_from_slice(&buf).unwrap_err();
    assert!(
      matches!(err, DecodeError::WireTypeMismatch { field_number: 2, expected, actual }
        if expected == VARINT && actual == LEN),
      "got {err:?}"
    );
  }

  #[test]
  fn timebase_den_zero_is_clamped_to_one() {
    let mut buf: Vec<u8> = Vec::new();
    Tag::new(1, WireType::Varint).encode(&mut buf);
    encode_uint32(7, &mut buf);
    Tag::new(2, WireType::Varint).encode(&mut buf);
    encode_uint32(0, &mut buf); // malformed den == 0 on the wire
    let tb = <Timebase as Message>::decode_from_slice(&buf).expect("decodes with clamp");
    assert_eq!(tb.num(), 7);
    assert_eq!(tb.den().get(), 1);
  }

  #[test]
  fn timebase_unknown_field_is_skipped() {
    let mut buf: Vec<u8> = Vec::new();
    Tag::new(1, WireType::Varint).encode(&mut buf);
    encode_uint32(2, &mut buf);
    Tag::new(2, WireType::Varint).encode(&mut buf);
    encode_uint32(3, &mut buf);
    Tag::new(7, WireType::Varint).encode(&mut buf); // unknown field → skip_field_depth
    encode_varint(99, &mut buf);
    let tb = <Timebase as Message>::decode_from_slice(&buf).expect("unknown field skipped");
    assert_eq!(tb, Timebase::new(2, nz(3)));
  }

  // ---- TimeRange ----

  #[test]
  fn timerange_default_instance_and_clear() {
    let di = <TimeRange as DefaultInstance>::default_instance();
    assert_eq!((di.start_pts(), di.end_pts()), (0, 0));
    assert_eq!(di.timebase(), Timebase::default());
    let mut r = TimeRange::new(3, 5, Timebase::new(2, nz(3)));
    Message::clear(&mut r);
    assert_eq!((r.start_pts(), r.end_pts()), (0, 0));
    assert_eq!(r.timebase(), Timebase::default());
  }

  #[test]
  fn timerange_wrong_wire_types_error() {
    let mut b1: Vec<u8> = Vec::new();
    Tag::new(1, WireType::LengthDelimited).encode(&mut b1);
    encode_varint(0, &mut b1);
    assert!(matches!(
      <TimeRange as Message>::decode_from_slice(&b1).unwrap_err(),
      DecodeError::WireTypeMismatch { field_number: 1, expected, actual }
        if expected == VARINT && actual == LEN
    ));
    let mut b2: Vec<u8> = Vec::new();
    Tag::new(2, WireType::LengthDelimited).encode(&mut b2);
    encode_varint(0, &mut b2);
    assert!(matches!(
      <TimeRange as Message>::decode_from_slice(&b2).unwrap_err(),
      DecodeError::WireTypeMismatch { field_number: 2, expected, actual }
        if expected == VARINT && actual == LEN
    ));
    let mut b3: Vec<u8> = Vec::new();
    Tag::new(3, WireType::Varint).encode(&mut b3);
    encode_varint(0, &mut b3);
    assert!(matches!(
      <TimeRange as Message>::decode_from_slice(&b3).unwrap_err(),
      DecodeError::WireTypeMismatch { field_number: 3, expected, actual }
        if expected == LEN && actual == VARINT
    ));
  }

  #[test]
  fn timerange_unknown_field_is_skipped() {
    let original = TimeRange::new(10, 20, Timebase::new(30000, nz(1001)));
    let mut buf = original.encode_to_vec();
    Tag::new(9, WireType::Varint).encode(&mut buf); // unknown → skip_field_depth
    encode_varint(123, &mut buf);
    let r = <TimeRange as Message>::decode_from_slice(&buf).expect("unknown field skipped");
    assert_eq!(r, original);
  }

  // ---- Timestamp ----

  #[test]
  fn timestamp_default_instance_and_clear() {
    let di = <Timestamp as DefaultInstance>::default_instance();
    assert_eq!(di.pts(), 0);
    assert_eq!(di.timebase(), Timebase::default());
    let mut ts = Timestamp::new(42, Timebase::new(2, nz(3)));
    Message::clear(&mut ts);
    assert_eq!(ts.pts(), 0);
    assert_eq!(ts.timebase(), Timebase::default());
  }

  #[test]
  fn timestamp_wrong_wire_types_error() {
    let mut b1: Vec<u8> = Vec::new();
    Tag::new(1, WireType::LengthDelimited).encode(&mut b1);
    encode_varint(0, &mut b1);
    assert!(matches!(
      <Timestamp as Message>::decode_from_slice(&b1).unwrap_err(),
      DecodeError::WireTypeMismatch { field_number: 1, expected, actual }
        if expected == VARINT && actual == LEN
    ));
    let mut b2: Vec<u8> = Vec::new();
    Tag::new(2, WireType::Varint).encode(&mut b2);
    encode_varint(0, &mut b2);
    assert!(matches!(
      <Timestamp as Message>::decode_from_slice(&b2).unwrap_err(),
      DecodeError::WireTypeMismatch { field_number: 2, expected, actual }
        if expected == LEN && actual == VARINT
    ));
  }

  #[test]
  fn timestamp_unknown_field_is_skipped() {
    let original = Timestamp::new(-99, Timebase::new(24000, nz(1001)));
    let mut buf = original.encode_to_vec();
    Tag::new(6, WireType::Varint).encode(&mut buf); // unknown → skip_field_depth
    encode_varint(7, &mut buf);
    let ts = <Timestamp as Message>::decode_from_slice(&buf).expect("unknown field skipped");
    assert_eq!(ts, original);
  }
}
