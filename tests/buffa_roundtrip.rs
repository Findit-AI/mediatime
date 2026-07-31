#![cfg(feature = "buffa")]

use buffa::Message;
use core::num::NonZeroI32;
use mediatime::{TimeRange, Timebase, Timestamp};

fn nz(n: i32) -> NonZeroI32 {
  NonZeroI32::new(n).unwrap()
}

#[test]
fn timebase_roundtrips() {
  for tb in [
    Timebase::new(30000, nz(1001)),
    Timebase::new(0, nz(1)),
    Timebase::new(1, nz(48000)),
  ] {
    let bytes = tb.encode_to_vec();
    let back = Timebase::decode_from_slice(&bytes).expect("decode");
    assert_eq!(tb, back, "Timebase round-trip failed");
  }
}

#[test]
fn timerange_roundtrips() {
  let tb = Timebase::new(1, nz(90000));
  for tr in [
    TimeRange::new(0, 0, tb),
    TimeRange::new(100, 250, tb),
    TimeRange::new(-5, 5, Timebase::new(30000, nz(1001))),
  ] {
    let bytes = tr.encode_to_vec();
    let back = TimeRange::decode_from_slice(&bytes).expect("decode");
    assert_eq!(tr, back, "TimeRange round-trip failed");
  }
}

#[test]
fn timestamp_roundtrips() {
  let tb = Timebase::new(1, nz(1000));
  for ts in [
    Timestamp::new(0, tb),
    Timestamp::new(123456, tb),
    Timestamp::new(-99, Timebase::new(24000, nz(1001))),
  ] {
    let bytes = ts.encode_to_vec();
    let back = Timestamp::decode_from_slice(&bytes).expect("decode");
    assert_eq!(ts, back, "Timestamp round-trip failed");
  }
}
