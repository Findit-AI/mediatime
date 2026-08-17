use super::*;
use serde::{
  Deserialize,
  de::value::{Error, MapDeserializer},
};

fn de(num: i32, den: i32) -> Result<Timebase, Error> {
  Timebase::deserialize(MapDeserializer::new(
    [("numerator", num), ("denominator", den)].into_iter(),
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

const fn nz(n: i32) -> NonZeroI32 {
  match NonZeroI32::new(n) {
    Some(v) => v,
    None => panic!("zero"),
  }
}
