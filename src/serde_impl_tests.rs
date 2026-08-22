use super::*;
use serde::{
  Deserialize, Deserializer,
  de::{
    IntoDeserializer, Visitor,
    value::{Error, MapDeserializer},
  },
  forward_to_deserialize_any,
};

fn de(num: i32, den: i32) -> Result<Timebase, Error> {
  Timebase::deserialize(MapDeserializer::new(
    [("numerator", num), ("denominator", den)].into_iter(),
  ))
}

/// The two value shapes the composite maps hold: an integer field and a
/// nested timebase map. serde's `de::value` helpers only build *homogeneous*
/// maps, and no self-describing format is among this crate's dev-dependencies,
/// so the heterogeneous map is built here instead.
enum Field {
  Integer(i64),
  Timebase(i32, i32),
}

impl IntoDeserializer<'_, Error> for Field {
  type Deserializer = Self;

  fn into_deserializer(self) -> Self {
    self
  }
}

impl<'de> Deserializer<'de> for Field {
  type Error = Error;

  fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
    match self {
      Self::Integer(v) => visitor.visit_i64(v),
      Self::Timebase(num, den) => visitor.visit_map(MapDeserializer::new(
        [("numerator", num), ("denominator", den)].into_iter(),
      )),
    }
  }

  forward_to_deserialize_any! {
    bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
    bytes byte_buf option unit unit_struct newtype_struct seq tuple
    tuple_struct map struct enum identifier ignored_any
  }
}

fn de_span(ticks: i64, num: i32, den: i32) -> Result<SignedDuration, Error> {
  SignedDuration::deserialize(MapDeserializer::new(
    [
      ("ticks", Field::Integer(ticks)),
      ("timebase", Field::Timebase(num, den)),
    ]
    .into_iter(),
  ))
}

fn de_duration(ticks: i64, num: i32, den: i32) -> Result<Duration, Error> {
  Duration::deserialize(MapDeserializer::new(
    [
      ("ticks", Field::Integer(ticks)),
      ("timebase", Field::Timebase(num, den)),
    ]
    .into_iter(),
  ))
}

fn de_range(start: i64, end: i64, num: i32, den: i32) -> Result<TimeRange, Error> {
  TimeRange::deserialize(MapDeserializer::new(
    [
      ("start", Field::Integer(start)),
      ("end", Field::Integer(end)),
      ("timebase", Field::Timebase(num, den)),
    ]
    .into_iter(),
  ))
}

#[test]
fn deserialize_accepts_the_values_the_constructor_accepts() {
  assert_eq!(de(30_000, 1001).unwrap(), Timebase::new(30_000, nz(1001)));
  assert_eq!(de(0, 3).unwrap(), Timebase::new(0, nz(3)));
  assert_eq!(
    de(i32::MAX, i32::MAX).unwrap(),
    Timebase::new(i32::MAX, nz(i32::MAX))
  );
}

#[test]
fn deserialize_rejects_what_the_constructor_rejects() {
  // The derive assigns fields directly; without the field validators these
  // would mint a `Timebase` that `new` refuses, which the arithmetic's
  // sign assumptions depend on being impossible.
  assert!(de(-1, 1000).is_err());
  assert!(de(1, -1000).is_err());
  assert!(de(1, 0).is_err());
}

#[test]
fn field_names_are_unchanged() {
  // The wire names are the compatibility surface; the field *types* moved
  // but `numerator`/`denominator` must not.
  let by_wrong_name: Result<Timebase, Error> =
    Timebase::deserialize(MapDeserializer::new([("num", 1), ("den", 2)].into_iter()));
  assert!(by_wrong_name.is_err());
}

#[test]
fn rate_deserialize_is_its_rational() {
  // A newtype is transparent on the wire, so a rate arrives as the rational
  // it is — under the `Timebase` field names, with the `Timebase` validators.
  fn de_rate(num: i32, den: i32) -> Result<Rate, Error> {
    Rate::deserialize(MapDeserializer::new(
      [("numerator", num), ("denominator", den)].into_iter(),
    ))
  }

  assert_eq!(de_rate(30_000, 1001).unwrap(), Rate::FPS_29_97);
  assert_eq!(de_rate(0, 1).unwrap(), Rate::hz(0));
  assert!(de_rate(-1, 1001).is_err());
  assert!(de_rate(30_000, 0).is_err());
  assert!(de_rate(30_000, -1001).is_err());
}

#[test]
fn signed_duration_deserialize_admits_both_directions() {
  let ms = Timebase::new(1, nz(1000));
  assert_eq!(
    de_span(-1500, 1, 1000).unwrap(),
    SignedDuration::new(-1500, ms)
  );
  assert_eq!(
    de_span(1500, 1, 1000).unwrap(),
    SignedDuration::new(1500, ms)
  );
  // The count has no invariant to enforce — a span points either way — but
  // the nested timebase keeps its own field validators.
  assert!(de_span(0, -1, 1000).is_err());
  assert!(de_span(0, 1, 0).is_err());
  assert!(de_span(0, 1, -1000).is_err());
}

#[test]
fn signed_duration_field_names_are_unchanged() {
  // Both names are the compatibility surface, and both are required.
  let renamed: Result<SignedDuration, Error> = SignedDuration::deserialize(MapDeserializer::new(
    [
      ("count", Field::Integer(0)),
      ("timebase", Field::Timebase(1, 1000)),
    ]
    .into_iter(),
  ));
  assert!(renamed.is_err());

  let missing_timebase: Result<SignedDuration, Error> = SignedDuration::deserialize(
    MapDeserializer::new([("ticks", Field::Integer(0))].into_iter()),
  );
  assert!(missing_timebase.is_err());
}

#[test]
fn duration_deserialize_admits_the_full_unsigned_range() {
  let ms = Timebase::new(1, nz(1000));
  assert_eq!(de_duration(1500, 1, 1000).unwrap(), Duration::new(1500, ms));
  assert_eq!(de_duration(0, 1, 1000).unwrap(), Duration::new(0, ms));
  // The count has no invariant to enforce, exactly as `SignedDuration`'s
  // does not, but the nested timebase keeps its own field validators.
  assert!(de_duration(0, -1, 1000).is_err());
  assert!(de_duration(0, 1, 0).is_err());
  assert!(de_duration(0, 1, -1000).is_err());
}

#[test]
fn duration_field_names_are_unchanged() {
  // Both names are the compatibility surface, and both are required.
  let renamed: Result<Duration, Error> = Duration::deserialize(MapDeserializer::new(
    [
      ("count", Field::Integer(0)),
      ("timebase", Field::Timebase(1, 1000)),
    ]
    .into_iter(),
  ));
  assert!(renamed.is_err());

  let missing_timebase: Result<Duration, Error> = Duration::deserialize(MapDeserializer::new(
    [("ticks", Field::Integer(0))].into_iter(),
  ));
  assert!(missing_timebase.is_err());
}

#[test]
fn time_range_deserialize_accepts_what_the_constructor_accepts() {
  let ms = Timebase::new(1, nz(1000));
  assert_eq!(
    de_range(1500, 3250, 1, 1000).unwrap(),
    TimeRange::new(1500, 3250, ms)
  );
  // Degenerate instant, and a negative start (pre-roll) both stay legal.
  assert_eq!(
    de_range(42, 42, 1, 1000).unwrap(),
    TimeRange::new(42, 42, ms)
  );
  assert_eq!(
    de_range(-1500, 0, 1, 1000).unwrap(),
    TimeRange::new(-1500, 0, ms)
  );
}

#[test]
fn time_range_deserialize_rejects_backwards_endpoints() {
  // `start <= end` spans two fields, so no per-field validator can see it.
  // Without the whole-struct check this decoded, and `TimeRange::duration`
  // then panicked on a value that `new` and `try_new` both refuse.
  assert!(de_range(3250, 1500, 1, 1000).is_err());
  assert!(de_range(0, i64::MIN, 1, 1000).is_err());
}

#[test]
fn time_range_deserialize_still_validates_its_timebase() {
  // The nested `Timebase` keeps its own field validators through the
  // intermediate representation.
  assert!(de_range(0, 10, -1, 1000).is_err());
  assert!(de_range(0, 10, 1, 0).is_err());
  assert!(de_range(0, 10, 1, -1000).is_err());
}

#[test]
fn time_range_field_names_are_unchanged() {
  // The intermediate representation is invisible on the wire: same three
  // names, all still required.
  let renamed: Result<TimeRange, Error> = TimeRange::deserialize(MapDeserializer::new(
    [
      ("from", Field::Integer(0)),
      ("to", Field::Integer(10)),
      ("timebase", Field::Timebase(1, 1000)),
    ]
    .into_iter(),
  ));
  assert!(renamed.is_err());

  let missing_end: Result<TimeRange, Error> = TimeRange::deserialize(MapDeserializer::new(
    [
      ("start", Field::Integer(0)),
      ("timebase", Field::Timebase(1, 1000)),
    ]
    .into_iter(),
  ));
  assert!(missing_end.is_err());
}

#[test]
fn time_range_decode_preserves_every_field() {
  // The intermediate representation must hand back what arrived, unreduced
  // and unrescaled. Compared through `Debug` because `==` on this type is
  // partly semantic: `30000/1001` and `60000/2002` are one timebase to it.
  let original = TimeRange::new(1500, 3250, Timebase::new(30_000, nz(1001)));
  let decoded = de_range(1500, 3250, 30_000, 1001).expect("decodes");
  assert_eq!(format!("{decoded:?}"), format!("{original:?}"));
}

const fn nz(n: i32) -> NonZeroI32 {
  match NonZeroI32::new(n) {
    Some(v) => v,
    None => panic!("zero"),
  }
}
