use super::*;
use quickcheck::{Arbitrary, Gen};

const ITERATIONS: usize = 1000;
const SIZE: usize = 512;

#[test]
fn timebase_denominator_is_nonzero() {
  let mut g = Gen::new(SIZE);
  for _ in 0..ITERATIONS {
    let tb = Timebase::arbitrary(&mut g);
    assert!(tb.den().get() > 0, "den was {}", tb.den());
    assert!(tb.num() >= 0, "num was {}", tb.num());
  }
}

#[test]
fn timestamp_pts_is_non_negative() {
  let mut g = Gen::new(SIZE);
  for _ in 0..ITERATIONS {
    let ts = Timestamp::arbitrary(&mut g);
    assert!(ts.pts() >= 0, "pts was {}", ts.pts());
    assert!(ts.timebase().den().get() > 0);
    assert!(ts.timebase().num() >= 0);
  }
}

#[test]
fn timerange_is_well_formed() {
  let mut g = Gen::new(SIZE);
  for _ in 0..ITERATIONS {
    let r = TimeRange::arbitrary(&mut g);
    assert!(r.start_pts() >= 0, "start was {}", r.start_pts());
    assert!(r.end_pts() >= 0, "end was {}", r.end_pts());
    assert!(
      r.start_pts() <= r.end_pts(),
      "start {} > end {}",
      r.start_pts(),
      r.end_pts()
    );
    assert!(r.timebase().den().get() > 0);
    assert!(r.timebase().num() >= 0);
  }
}
