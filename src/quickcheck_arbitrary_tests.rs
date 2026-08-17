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
fn rate_is_a_well_formed_rational() {
  let mut g = Gen::new(SIZE);
  for _ in 0..ITERATIONS {
    let rate = Rate::arbitrary(&mut g);
    assert!(rate.den().get() > 0, "den was {}", rate.den());
    assert!(rate.num() >= 0, "num was {}", rate.num());
    // The reciprocal exists for exactly the non-degenerate draws.
    assert_eq!(rate.checked_to_timebase().is_some(), rate.num() != 0);
  }
}

#[test]
fn signed_duration_spans_both_directions() {
  let mut g = Gen::new(SIZE);
  let mut backwards = false;
  let mut forwards = false;
  for _ in 0..ITERATIONS {
    let span = SignedDuration::arbitrary(&mut g);
    assert!(span.timebase().den().get() > 0);
    assert!(span.timebase().num() >= 0);
    backwards |= span.is_negative();
    forwards |= span.is_positive();
  }
  // A span has no non-negativity to preserve, and this generator must not
  // quietly become the instant one next door, which has.
  assert!(backwards && forwards);
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

/// The table-driven round trips in `parse::tests` pick their inputs; these
/// let the generator pick, over the whole domain each `Arbitrary` covers.
/// `Debug` is the structural comparison — `==` is semantic here, so it would
/// not catch a parser that reduced or rescaled.
#[test]
fn every_exact_rendering_parses_back_to_the_value_that_wrote_it() {
  let mut g = Gen::new(SIZE);
  for _ in 0..ITERATIONS {
    let tb = Timebase::arbitrary(&mut g);
    let parsed: Timebase = format!("{tb:#}").parse().expect("timebase parses back");
    assert_eq!(format!("{parsed:?}"), format!("{tb:?}"));

    let ts = Timestamp::arbitrary(&mut g);
    let parsed: Timestamp = format!("{ts:#}").parse().expect("timestamp parses back");
    assert_eq!(format!("{parsed:?}"), format!("{ts:?}"));

    let r = TimeRange::arbitrary(&mut g);
    let parsed: TimeRange = format!("{r:#}").parse().expect("time range parses back");
    assert_eq!(format!("{parsed:?}"), format!("{r:?}"));
  }
}
