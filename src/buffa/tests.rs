use super::*;

fn nz(n: i32) -> NonZeroI32 {
  NonZeroI32::new(n).unwrap()
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
  encode_int32(5, &mut buf);
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
  encode_int32(7, &mut buf);
  Tag::new(2, WireType::Varint).encode(&mut buf);
  encode_int32(0, &mut buf); // malformed den == 0 on the wire
  let tb = <Timebase as Message>::decode_from_slice(&buf).expect("decodes with clamp");
  assert_eq!(tb.num(), 7);
  assert_eq!(tb.den().get(), 1);
}

#[test]
fn timebase_negative_fields_are_clamped() {
  // A peer writing `uint32` values above `i32::MAX` produces varints that
  // `decode_int32` truncates into the negative half. Both fields clamp to
  // their smallest legal value rather than panicking in `Timebase::new`.
  let mut buf: Vec<u8> = Vec::new();
  Tag::new(1, WireType::Varint).encode(&mut buf);
  encode_int32(-7, &mut buf);
  Tag::new(2, WireType::Varint).encode(&mut buf);
  encode_int32(-9, &mut buf);
  let tb = <Timebase as Message>::decode_from_slice(&buf).expect("decodes with clamp");
  assert_eq!(tb.num(), 0);
  assert_eq!(tb.den().get(), 1);
}

#[test]
fn timebase_wire_bytes_are_unchanged_by_the_signed_fields() {
  // Golden bytes captured from the `uint32` encoding this type used before
  // `num`/`den` became signed. `int32` and `uint32` are the same plain
  // varint for non-negative values, so the encoding must not have moved —
  // and old bytes must still decode to the same value.
  for (tb, golden) in [
    (
      Timebase::new(30_000, nz(1001)),
      &b"\x08\xb0\xea\x01\x10\xe9\x07"[..],
    ),
    (Timebase::new(0, nz(1)), &b"\x08\x00\x10\x01"[..]),
    (
      Timebase::new(1, nz(48_000)),
      &b"\x08\x01\x10\x80\xf7\x02"[..],
    ),
    (Timebase::new(1, nz(1)), &b"\x08\x01\x10\x01"[..]),
    (
      Timebase::new(i32::MAX, nz(i32::MAX)),
      &b"\x08\xff\xff\xff\xff\x07\x10\xff\xff\xff\xff\x07"[..],
    ),
  ] {
    assert_eq!(tb.encode_to_vec(), golden, "encoding moved for {tb:?}");
    assert_eq!(
      <Timebase as Message>::decode_from_slice(golden).expect("golden decodes"),
      tb
    );
  }
}

#[test]
fn timebase_unknown_field_is_skipped() {
  let mut buf: Vec<u8> = Vec::new();
  Tag::new(1, WireType::Varint).encode(&mut buf);
  encode_int32(2, &mut buf);
  Tag::new(2, WireType::Varint).encode(&mut buf);
  encode_int32(3, &mut buf);
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
