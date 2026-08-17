use super::*;
use arbitrary::{Arbitrary, Unstructured};

fn pseudo_random_bytes(seed: u64) -> [u8; 4096] {
  // splitmix64: simple, reproducible, good enough for fuzz-input fodder.
  let mut out = [0u8; 4096];
  let mut s = seed.wrapping_add(0x9E3779B97F4A7C15);
  for chunk in out.chunks_mut(8) {
    s = s.wrapping_mul(0xBF58476D1CE4E5B9).wrapping_add(1);
    let mut z = s;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^= z >> 31;
    chunk.copy_from_slice(&z.to_le_bytes()[..chunk.len()]);
  }
  out
}

#[test]
fn timebase_denominator_is_nonzero() {
  for seed in 0..200 {
    let data = pseudo_random_bytes(seed);
    let mut u = Unstructured::new(&data);
    let tb = Timebase::arbitrary(&mut u).expect("enough bytes to build a Timebase");
    assert!(tb.den().get() > 0, "den was {}", tb.den());
    assert!(tb.num() >= 0, "num was {}", tb.num());
  }
}

#[test]
fn timestamp_pts_is_non_negative() {
  for seed in 0..200 {
    let data = pseudo_random_bytes(seed);
    let mut u = Unstructured::new(&data);
    let ts = Timestamp::arbitrary(&mut u).expect("enough bytes to build a Timestamp");
    assert!(ts.pts() >= 0, "pts was {}", ts.pts());
    assert!(ts.timebase().den().get() > 0);
    assert!(ts.timebase().num() >= 0);
  }
}

#[test]
fn timerange_is_well_formed() {
  for seed in 0..200 {
    let data = pseudo_random_bytes(seed);
    let mut u = Unstructured::new(&data);
    let r = TimeRange::arbitrary(&mut u).expect("enough bytes to build a TimeRange");
    assert!(r.start_pts() >= 0);
    assert!(r.end_pts() >= 0);
    assert!(r.start_pts() <= r.end_pts());
    assert!(r.timebase().den().get() > 0);
    assert!(r.timebase().num() >= 0);
  }
}

#[test]
fn arbitrary_take_rest_produces_valid_values() {
  // `arbitrary_take_rest` is the entry point fuzzers use at the tail of
  // a corpus entry. Make sure our impl plays well with it.
  let data = pseudo_random_bytes(42);
  let u = Unstructured::new(&data);
  let r = TimeRange::arbitrary_take_rest(u).expect("should consume the buffer");
  assert!(r.start_pts() >= 0);
  assert!(r.end_pts() >= 0);
  assert!(r.start_pts() <= r.end_pts());
  assert!(r.timebase().den().get() > 0);
  assert!(r.timebase().num() >= 0);
}
