use super::*;

fn hash_of<T: Hash>(v: &T) -> u64 {
  use std::collections::hash_map::DefaultHasher;
  let mut h = DefaultHasher::new();
  v.hash(&mut h);
  h.finish()
}

#[test]
fn rescale_identity() {
  let tb = Timebase::new(1, nz(1000));
  assert_eq!(tb.checked_rescale(42, tb), Some(42));
  assert_eq!(tb.saturating_rescale(42, tb), 42);
}

#[test]
fn rescale_between_timebases() {
  let ms = Timebase::MILLIS;
  let mpeg = Timebase::MPEG_90K;
  assert_eq!(ms.checked_rescale(1000, mpeg), Some(90_000));
  assert_eq!(ms.saturating_rescale(1000, mpeg), 90_000);
  assert_eq!(mpeg.checked_rescale(90_000, ms), Some(1000));
}

#[test]
fn rescale_rounds_to_nearest() {
  // 1 ms is 0.003 of a 1/3-second tick — nearest is still zero, at both
  // signs.
  let ms = Timebase::MILLIS;
  let thirds = Timebase::new(1, nz(3));
  assert_eq!(ms.checked_rescale(1, thirds), Some(0));
  assert_eq!(ms.checked_rescale(-1, thirds), Some(0));

  // 400 ms is 1.2 ticks and 600 ms is 1.8 — truncation would call both 1.
  assert_eq!(ms.checked_rescale(400, thirds), Some(1));
  assert_eq!(ms.checked_rescale(600, thirds), Some(2));
  assert_eq!(ms.checked_rescale(-400, thirds), Some(-1));
  assert_eq!(ms.checked_rescale(-600, thirds), Some(-2));
}

#[test]
fn rescale_breaks_ties_away_from_zero() {
  // 500 ms is exactly 1.5 ticks of 1/3 s. `AV_ROUND_NEAR_INF` — FFmpeg's
  // default for `av_rescale` and `av_rescale_q`, and this crate's — sends it
  // to 2, and its mirror to -2; rounding half *up* would have said -1.
  let ms = Timebase::MILLIS;
  let thirds = Timebase::new(1, nz(3));
  assert_eq!(ms.checked_rescale(500, thirds), Some(2));
  assert_eq!(ms.checked_rescale(-500, thirds), Some(-2));
  assert_eq!(ms.saturating_rescale(500, thirds), 2);
  assert_eq!(ms.saturating_rescale(-500, thirds), -2);

  // An odd count of half-seconds into whole seconds is the smallest tie
  // there is, and the one an odd denominator cannot express.
  let half = Timebase::new(1, nz(2));
  assert_eq!(half.checked_rescale(1, Timebase::SECONDS), Some(1));
  assert_eq!(half.checked_rescale(-1, Timebase::SECONDS), Some(-1));
  assert_eq!(half.checked_rescale(3, Timebase::SECONDS), Some(2));
  assert_eq!(half.checked_rescale(-3, Timebase::SECONDS), Some(-2));
}

#[test]
fn rescale_saturates_or_refuses_on_i64_overflow() {
  // Rescale from a coarse timebase (i32::MAX seconds per tick) to a fine
  // one (1/i32::MAX seconds per tick): even a modest pts blows past
  // i64::MAX in the 128-bit intermediate. The checked rung refuses; the
  // saturating one clamps rather than wrapping via `as i64`.
  let from = Timebase::new(i32::MAX, nz(1));
  let to = Timebase::new(1, nz(i32::MAX));
  assert_eq!(from.checked_rescale(1_000_000, to), None);
  assert_eq!(from.checked_rescale(-1_000_000, to), None);
  assert_eq!(from.saturating_rescale(1_000_000, to), i64::MAX);
  assert_eq!(from.saturating_rescale(-1_000_000, to), i64::MIN);
}

#[test]
fn rescale_refuses_a_degenerate_target() {
  // A degenerate timebase names one single instant, so no tick count in it
  // can stand for another one. The checked rung says so.
  let degenerate = Timebase::new(0, nz(3));
  assert_eq!(Timebase::MILLIS.checked_rescale(1000, degenerate), None);
  assert_eq!(Timebase::MILLIS.checked_rescale(0, degenerate), None);

  // A degenerate *source* is not a failure: every tick of it is instant
  // zero, and zero is representable in any target.
  assert_eq!(degenerate.checked_rescale(999, Timebase::MILLIS), Some(0));
  assert_eq!(degenerate.saturating_rescale(999, Timebase::MILLIS), 0);
}

#[test]
#[should_panic(expected = "target timebase numerator must be non-zero")]
fn saturating_rescale_panics_on_a_degenerate_target() {
  // `i64::saturating_div`'s posture: saturation answers overflow, not a zero
  // divisor. `checked_rescale` is the total spelling.
  Timebase::MILLIS.saturating_rescale(1000, Timebase::new(0, nz(3)));
}

#[test]
fn reduce_lands_in_lowest_terms() {
  let coarse = Timebase::new(2, nz(4));
  let reduced = coarse.reduce();
  assert_eq!(reduced.num(), 1);
  assert_eq!(reduced.den().get(), 2);
  // The value never moved, so the reduced form hashes with what it came from.
  assert_eq!(reduced, coarse);
  assert_eq!(hash_of(&reduced), hash_of(&coarse));

  // A degenerate timebase has a canonical form too, and it is 0/1.
  let degenerate = Timebase::new(0, nz(3)).reduce();
  assert_eq!(degenerate.num(), 0);
  assert_eq!(degenerate.den().get(), 1);

  // The widest input: gcd(i32::MAX, i32::MAX) is i32::MAX.
  let max = Timebase::new(i32::MAX, nz(i32::MAX)).reduce();
  assert_eq!(max.num(), 1);
  assert_eq!(max.den().get(), 1);

  // Already-reduced values come back untouched, 1001-series included.
  for tb in [Timebase::NTSC_VIDEO, Timebase::MPEG_90K] {
    assert_eq!(format!("{:?}", tb.reduce()), format!("{tb:?}"));
  }
}

#[test]
fn is_reduced_agrees_with_reduce() {
  for (tb, expected) in [
    (Timebase::new(1, nz(2)), true),
    (Timebase::new(2, nz(4)), false),
    (Timebase::new(0, nz(1)), true),
    (Timebase::new(0, nz(3)), false),
    (Timebase::new(1_001, nz(30_000)), true),
    (Timebase::new(2_002, nz(60_000)), false),
  ] {
    assert_eq!(tb.is_reduced(), expected, "{tb}");
    assert!(tb.reduce().is_reduced(), "{tb}");
  }
}

#[test]
fn checked_recip_swaps_the_halves() {
  let film = Timebase::FILM_24;
  let fps = film.checked_recip().expect("24 fps has a reciprocal");
  assert_eq!(fps.num(), 24);
  assert_eq!(fps.den().get(), 1);
  // The reciprocal read as a frame rate is the rate the constant is named for.
  assert_eq!(fps.frames_to_duration(24), Duration::from_secs(1));

  // Round trip, structurally: nothing is reduced or normalized on the way.
  let ntsc = Timebase::new(1_001, nz(30_000));
  let there = ntsc.checked_recip().expect("has a reciprocal");
  assert_eq!(there.num(), 30_000);
  assert_eq!(there.den().get(), 1_001);
  assert_eq!(
    format!("{:?}", there.checked_recip().expect("and back")),
    format!("{ntsc:?}")
  );

  // The single failure: a zero numerator is not a denominator.
  assert_eq!(Timebase::new(0, nz(3)).checked_recip(), None);
}

#[test]
fn timebase_eq_is_semantic() {
  // 1/2 == 2/4 == 3/6
  let a = Timebase::new(1, nz(2));
  let b = Timebase::new(2, nz(4));
  let c = Timebase::new(3, nz(6));
  assert_eq!(a, b);
  assert_eq!(b, c);
  assert_eq!(a, c);
  // 1/2 != 1/3
  let d = Timebase::new(1, nz(3));
  assert_ne!(a, d);
}

#[test]
fn timebase_hash_matches_eq() {
  let a = Timebase::new(1, nz(2));
  let b = Timebase::new(2, nz(4));
  let c = Timebase::new(3, nz(6));
  assert_eq!(hash_of(&a), hash_of(&b));
  assert_eq!(hash_of(&b), hash_of(&c));
}

#[test]
fn timebase_ord_is_numeric() {
  let third = Timebase::new(1, nz(3));
  let half = Timebase::new(1, nz(2));
  let two_thirds = Timebase::new(2, nz(3));
  let one = Timebase::new(1, nz(1));
  assert!(third < half);
  assert!(half < two_thirds);
  assert!(two_thirds < one);
  // Structural lex order would have reported (1, 1) < (1, 3); verify it doesn't.
  assert!(one > third);
}

#[test]
fn timebase_num_zero() {
  // 0/3 == 0/5, and both compare less than anything positive.
  let a = Timebase::new(0, nz(3));
  let b = Timebase::new(0, nz(5));
  assert_eq!(a, b);
  assert_eq!(hash_of(&a), hash_of(&b));
  assert!(a < Timebase::new(1, nz(1_000_000)));
}

#[test]
fn timebase_zero_denominator_stays_unrepresentable() {
  // `NonZeroI32` still carries the non-zero half of the invariant, so a
  // zero denominator cannot reach `new` at all — only the *sign* half moved
  // into the constructor.
  assert!(NonZeroI32::new(0).is_none());
}

#[test]
fn timebase_rejects_negative_denominator() {
  assert!(Timebase::try_new(1, nz(-1000)).is_none());
  assert!(Timebase::try_new(1, nz(i32::MIN)).is_none());
}

#[test]
fn timebase_rejects_negative_numerator() {
  assert!(Timebase::try_new(-1, nz(1000)).is_none());
  assert!(Timebase::try_new(i32::MIN, nz(1000)).is_none());
}

#[test]
fn timebase_accepts_zero_numerator_and_i32_max() {
  // A zero numerator is a degenerate but legal timebase, and both fields
  // must reach the top of their range — the whole point of the type change
  // is that `i32::MAX` round-trips into an `AVRational`.
  assert!(Timebase::try_new(0, nz(3)).is_some());
  let max = Timebase::try_new(i32::MAX, nz(i32::MAX)).expect("i32::MAX is legal at both ends");
  assert_eq!(max.num(), i32::MAX);
  assert_eq!(max.den().get(), i32::MAX);
  assert_eq!(max, Timebase::new(i32::MAX, nz(i32::MAX)));
}

#[test]
#[should_panic(expected = "timebase numerator must not be negative")]
fn timebase_new_panics_on_negative_numerator() {
  Timebase::new(-1, nz(1000));
}

#[test]
#[should_panic(expected = "timebase denominator must be positive")]
fn timebase_new_panics_on_negative_denominator() {
  Timebase::new(1, nz(-1000));
}

#[test]
#[should_panic(expected = "timebase numerator must not be negative")]
fn timebase_set_num_panics_on_negative() {
  Timebase::default().with_num(-1);
}

#[test]
#[should_panic(expected = "timebase denominator must be positive")]
fn timebase_set_den_panics_on_negative() {
  Timebase::default().with_den(nz(-1));
}

#[test]
fn timebase_is_const_constructible() {
  // The panics added to `new` must not have cost the type its `const`
  // constructor, which the crate advertises.
  const TB: Timebase = Timebase::new(30_000, nz(1001));
  const NUM: i32 = TB.num();
  const TRIED: Option<Timebase> = Timebase::try_new(-1, DEN_ONE);
  assert_eq!(NUM, 30_000);
  assert!(TRIED.is_none());
}

#[test]
fn timestamp_cmp_same_timebase() {
  let tb = Timebase::new(1, nz(1000));
  let a = Timestamp::new(100, tb);
  let b = Timestamp::new(200, tb);
  assert!(a < b);
  assert!(b > a);
  assert_eq!(a, a);
  assert_eq!(a.cmp(&b), Ordering::Less);
}

#[test]
fn timestamp_cmp_cross_timebase() {
  let a = Timestamp::new(1000, Timebase::new(1, nz(1000)));
  let b = Timestamp::new(90_000, Timebase::new(1, nz(90_000)));
  assert_eq!(a, b);
  assert_eq!(a.cmp(&b), Ordering::Equal);

  let c = Timestamp::new(500, Timebase::new(1, nz(1000)));
  assert!(c < a);
  assert!(a > c);
}

#[test]
fn timestamp_hash_matches_semantic_eq() {
  let a = Timestamp::new(1000, Timebase::new(1, nz(1000)));
  let b = Timestamp::new(90_000, Timebase::new(1, nz(90_000)));
  let c = Timestamp::new(2000, Timebase::new(1, nz(2000))); // also 1.0s
  assert_eq!(a, b);
  assert_eq!(hash_of(&a), hash_of(&b));
  assert_eq!(hash_of(&a), hash_of(&c));
}

#[test]
fn timestamp_hash_negative_pts() {
  // Pre-roll / edit list scenarios: -500 ms should equal -45_000 @ 1/90_000.
  let a = Timestamp::new(-500, Timebase::new(1, nz(1000)));
  let b = Timestamp::new(-45_000, Timebase::new(1, nz(90_000)));
  assert_eq!(a, b);
  assert_eq!(hash_of(&a), hash_of(&b));
}

#[test]
fn rescale_to_preserves_instant() {
  let ms = Timebase::new(1, nz(1000));
  let mpeg = Timebase::new(1, nz(90_000));
  let a = Timestamp::new(1000, ms);
  let b = a.rescale_to(mpeg);
  assert_eq!(b.pts(), 90_000);
  assert_eq!(b.timebase(), mpeg);
  assert_eq!(a, b);
}

#[test]
fn timestamp_duration_from_zero() {
  let ms = Timebase::new(1, nz(1000));
  let ts = Timestamp::new(1500, ms);
  assert_eq!(ts.duration(), Some(Duration::from_millis(1500)));
  assert_eq!(Timestamp::new(0, ms).duration(), Some(Duration::ZERO));

  // Cross-timebase equivalence: same instant, same duration.
  let mpeg = Timebase::new(1, nz(90_000));
  assert_eq!(
    Timestamp::new(90_000, mpeg).duration(),
    Some(Duration::from_secs(1))
  );

  // Negative PTS (pre-roll) has no Duration representation.
  assert_eq!(Timestamp::new(-1, ms).duration(), None);
}

#[test]
fn duration_since_same_timebase() {
  let tb = Timebase::new(1, nz(1000));
  let a = Timestamp::new(1500, tb);
  let b = Timestamp::new(500, tb);
  assert_eq!(a.duration_since(&b), Some(Duration::from_millis(1000)));
  assert_eq!(b.duration_since(&a), None);
}

#[test]
fn duration_since_cross_timebase() {
  let a = Timestamp::new(1000, Timebase::new(1, nz(1000)));
  let b = Timestamp::new(45_000, Timebase::new(1, nz(90_000)));
  assert_eq!(a.duration_since(&b), Some(Duration::from_millis(500)));
}

#[test]
fn duration_since_saturates_to_duration_max_on_overflow() {
  // Use a timebase of `i32::MAX / 1` (each tick ≈ 2^31 seconds). Then
  // i64::MAX ticks ≈ 2^94 seconds — far more than u64::MAX. Should
  // saturate to Duration::MAX rather than wrap when casting seconds to u64.
  let tb = Timebase::new(i32::MAX, nz(1));
  let huge = Timestamp::new(i64::MAX, tb);
  let zero = Timestamp::new(0, tb);
  assert_eq!(huge.duration_since(&zero), Some(Duration::MAX));
}

#[test]
fn frames_to_duration_integer_fps() {
  let fps30 = Timebase::new(30, nz(1));
  assert_eq!(fps30.frames_to_duration(15), Duration::from_millis(500));
  assert_eq!(fps30.frames_to_duration(30), Duration::from_secs(1));
  assert_eq!(fps30.frames_to_duration(0), Duration::ZERO);
}

#[test]
fn frames_to_duration_ntsc() {
  // 30000 frames @ 30000/1001 fps = exactly 1001 seconds.
  let ntsc = Timebase::new(30_000, nz(1001));
  assert_eq!(ntsc.frames_to_duration(30_000), Duration::from_secs(1001));
  // 15 frames at NTSC ≈ 500.5 ms.
  assert_eq!(
    ntsc.frames_to_duration(15),
    Duration::from_nanos(500_500_000),
  );
}

#[test]
fn time_range_basic() {
  let tb = Timebase::default().with_den(nz(1000)).with_num(1);
  let r = TimeRange::new(100, 500, tb);
  assert_eq!(r.start_pts(), 100);
  assert_eq!(r.end_pts(), 500);
  assert_eq!(r.timebase(), tb);
  assert_eq!(r.start(), Timestamp::new(100, tb));
  assert_eq!(r.end(), Timestamp::new(500, tb));
  assert!(!r.is_instant());
  assert_eq!(r.duration(), Duration::from_millis(400));
  // Interpolate: t=0 → start, t=1 → end, t=0.5 → midpoint.
  assert_eq!(r.interpolate(0.0).pts(), 100);
  assert_eq!(r.interpolate(1.0).pts(), 500);
  assert_eq!(r.interpolate(0.5).pts(), 300);
  // Out-of-range t is clamped.
  assert_eq!(r.interpolate(-1.0).pts(), 100);
  assert_eq!(r.interpolate(2.0).pts(), 500);

  let nr = r.with_timebase(Timebase::new(1, nz(2000)));
  assert_eq!(nr.timebase().den().get(), 2000);
  assert_eq!(nr.timebase().num(), 1);
}

#[test]
fn time_range_instant() {
  let tb = Timebase::new(1, nz(1000));
  let ts = Timestamp::new(123, tb);
  let r = TimeRange::instant(ts);
  assert!(r.is_instant());
  assert_eq!(r.start_pts(), 123);
  assert_eq!(r.end_pts(), 123);
  assert_eq!(r.duration(), Duration::ZERO);
}

// -------------------------------------------------------------------------
// Coverage top-ups — every public accessor, builder, and setter on the
// three types gets exercised at least once. Grouped per-type.
// -------------------------------------------------------------------------

#[test]
fn timebase_accessors_and_builders() {
  let tb = Timebase::new(30_000, nz(1001));
  assert_eq!(tb.num(), 30_000);
  assert_eq!(tb.den(), nz(1001));

  // with_num / with_den — consuming form.
  let tb2 = tb.with_num(48_000).with_den(nz(1));
  assert_eq!(tb2.num(), 48_000);
  assert_eq!(tb2.den(), nz(1));

  // set_num / set_den — in-place form. Returns &mut Self for chaining.
  let mut tb3 = Timebase::new(1, nz(1000));
  tb3.set_num(25).set_den(nz(2));
  assert_eq!(tb3.num(), 25);
  assert_eq!(tb3.den(), nz(2));
}

#[test]
fn the_well_known_roster_holds_the_rationals_it_names() {
  // Each constant pinned against the convention its doc names, so a typo in a
  // denominator is a failing test rather than a wrong timestamp downstream.
  for (tb, num, den) in [
    (Timebase::SECONDS, 1, 1),
    (Timebase::MILLIS, 1, 1_000),
    (Timebase::MICROS, 1, 1_000_000),
    (Timebase::NANOS, 1, 1_000_000_000),
    (Timebase::MPEG_90K, 1, 90_000),
    (Timebase::HZ_48K, 1, 48_000),
    (Timebase::HZ_44_1K, 1, 44_100),
    (Timebase::NTSC_FILM, 1_001, 24_000),
    (Timebase::NTSC_VIDEO, 1_001, 30_000),
    (Timebase::FILM_24, 1, 24),
    (Timebase::PAL_25, 1, 25),
  ] {
    assert_eq!(tb.num(), num, "{tb}");
    assert_eq!(tb.den().get(), den, "{tb}");
    // Stored in lowest terms, so `Display` prints the canonical rational and
    // `reduce` is a no-op on the roster.
    assert!(tb.is_reduced(), "{tb}");
  }

  // The frame-rate entries are the reciprocals of the rate they are named
  // for — the trap the roster's doc warns about.
  assert_eq!(
    Timebase::NTSC_VIDEO
      .checked_recip()
      .expect("has a reciprocal")
      .frames_to_duration(30_000),
    Duration::from_secs(1001)
  );
}

#[test]
fn well_known_timebases_are_pairwise_distinct() {
  // What makes `well_known_name` single-valued: two entries of equal value
  // would make the reverse lookup depend on table order.
  for (i, (name, tb)) in WELL_KNOWN.iter().enumerate() {
    for (other_name, other) in &WELL_KNOWN[i + 1..] {
      assert_ne!(tb, other, "{name} and {other_name} are the same rational");
    }
  }
}

#[test]
fn from_name_and_well_known_name_are_one_table_read_both_ways() {
  for (name, tb) in WELL_KNOWN {
    assert_eq!(Timebase::from_name(name), Some(*tb), "{name}");
    assert_eq!(tb.well_known_name(), Some(*name), "{name}");
  }
  assert_eq!(WELL_KNOWN.len(), 11);
}

#[test]
fn from_name_matches_exactly_and_nothing_else() {
  // `SCREAMING_SNAKE_CASE`, case-sensitively: the constant's own spelling and
  // no alias, so the name in a config file is greppable in this crate.
  for s in ["millis", "Millis", " MILLIS", "MILLIS ", "MS", "1/1000", ""] {
    assert_eq!(Timebase::from_name(s), None, "{s:?}");
  }
  assert_eq!(Timebase::from_name("MILLIS"), Some(Timebase::MILLIS));
}

#[test]
fn well_known_name_matches_by_value_not_by_spelling() {
  // `==` on this type is value-based, and so is the name lookup: a stream
  // that declared `2/2000` is counting milliseconds and answers to the name,
  // even though `Display` still prints what it declared.
  let declared = Timebase::new(2, nz(2000));
  assert_eq!(declared.well_known_name(), Some("MILLIS"));
  assert_eq!(format!("{declared}"), "2/2000");

  // Nothing outside the roster gets a name.
  assert_eq!(Timebase::new(1, nz(7)).well_known_name(), None);
  assert_eq!(Timebase::new(0, nz(3)).well_known_name(), None);
}

#[test]
fn duration_to_pts_happy_path_and_edge_cases() {
  // Integer conversion: 1.5 s @ 1/1000 → 1500 units.
  let ms = Timebase::MILLIS;
  assert_eq!(
    ms.checked_duration_to_pts(Duration::from_millis(1500)),
    Some(1500)
  );
  assert_eq!(ms.checked_duration_to_pts(Duration::ZERO), Some(0));

  // Non-ms timebase: 2 s @ 1/90_000 → 180_000 units.
  assert_eq!(
    Timebase::MPEG_90K.checked_duration_to_pts(Duration::from_secs(2)),
    Some(180_000)
  );

  // Saturation at i64::MAX when the count would overflow — and the checked
  // rung refusing where the saturating one clamps. One tick per second
  // against the longest `Duration` there is: ~1.8e19 ticks against 9.2e18.
  let seconds = Timebase::SECONDS;
  let huge = Duration::new(u64::MAX, 0);
  assert_eq!(seconds.checked_duration_to_pts(huge), None);
  assert_eq!(seconds.saturating_duration_to_pts(huge), i64::MAX);
}

#[test]
fn duration_to_pts_rounds_to_nearest() {
  // 1.5 ms is exactly half a millisecond tick past 1 — away from zero, so 2,
  // where the truncating conversion this replaced said 1.
  let ms = Timebase::MILLIS;
  assert_eq!(
    ms.checked_duration_to_pts(Duration::from_nanos(1_500_000)),
    Some(2)
  );
  assert_eq!(
    ms.checked_duration_to_pts(Duration::from_nanos(1_400_000)),
    Some(1)
  );
  assert_eq!(
    ms.checked_duration_to_pts(Duration::from_nanos(1_600_000)),
    Some(2)
  );
  // Sub-tick durations round to the nearest tick rather than vanishing.
  assert_eq!(ms.checked_duration_to_pts(Duration::from_nanos(1)), Some(0));
  assert_eq!(
    ms.checked_duration_to_pts(Duration::from_nanos(999_999)),
    Some(1)
  );
}

#[test]
fn duration_to_pts_parts_ways_with_its_twin_on_a_degenerate_timebase() {
  // Every tick of a `0/den` timebase is instant zero, so no count of them
  // spans a second. The checked rung says so; the saturating one answers 0,
  // which is the identity `Timestamp::saturating_add_duration` needs and is
  // the *right* answer only for a zero duration.
  let degenerate = Timebase::new(0, nz(3));
  assert_eq!(
    degenerate.checked_duration_to_pts(Duration::from_secs(1)),
    None
  );
  assert_eq!(degenerate.checked_duration_to_pts(Duration::ZERO), None);
  assert_eq!(
    degenerate.saturating_duration_to_pts(Duration::from_secs(1)),
    0
  );
}

#[test]
fn pts_to_duration_inverts_duration_to_pts() {
  let ms = Timebase::MILLIS;
  assert_eq!(
    ms.checked_pts_to_duration(1500),
    Some(Duration::from_millis(1500))
  );
  assert_eq!(ms.checked_pts_to_duration(0), Some(Duration::ZERO));
  assert_eq!(
    Timebase::MPEG_90K.checked_pts_to_duration(45_000),
    Some(Duration::from_millis(500))
  );

  // One MPEG tick is 11111.11… ns, and the nearest whole nanosecond is what
  // a `Duration` can hold.
  assert_eq!(
    Timebase::MPEG_90K.checked_pts_to_duration(1),
    Some(Duration::from_nanos(11_111))
  );

  // A degenerate timebase is *not* a failure in this direction: all its ticks
  // are instant zero, and zero is a `Duration`.
  assert_eq!(
    Timebase::new(0, nz(3)).checked_pts_to_duration(999),
    Some(Duration::ZERO)
  );
}

#[test]
fn pts_to_duration_saturates_at_both_ends_of_a_duration() {
  let ms = Timebase::MILLIS;
  // A negative PTS is ordinary (pre-roll, edit lists) and has no `Duration`;
  // the saturating rung clamps to the floor of the type.
  assert_eq!(ms.checked_pts_to_duration(-1), None);
  assert_eq!(ms.saturating_pts_to_duration(-1), Duration::ZERO);
  assert_eq!(ms.saturating_pts_to_duration(i64::MIN), Duration::ZERO);

  // Past the ceiling: i32::MAX seconds per tick, i64::MAX ticks — about 2^94
  // seconds against a u64 seconds field.
  let coarse = Timebase::new(i32::MAX, nz(1));
  assert_eq!(coarse.checked_pts_to_duration(i64::MAX), None);
  assert_eq!(coarse.saturating_pts_to_duration(i64::MAX), Duration::MAX);
}

#[test]
fn timestamp_accessors_and_builders() {
  let tb = Timebase::new(1, nz(1000));
  let mut ts = Timestamp::new(42, tb);
  assert_eq!(ts.pts(), 42);
  assert_eq!(ts.timebase(), tb);

  // with_pts — consuming form.
  let ts2 = ts.with_pts(777);
  assert_eq!(ts2.pts(), 777);

  // set_pts — in-place form, chainable.
  ts.set_pts(-5).set_pts(-6);
  assert_eq!(ts.pts(), -6);
}

#[test]
fn cmp_semantic_exercises_all_branches() {
  let tb_a = Timebase::new(1, nz(1000)); // ms
  let tb_b = Timebase::new(1, nz(90_000)); // MPEG-TS

  // Same-timebase fast path: Less / Greater / Equal.
  let a = Timestamp::new(100, tb_a);
  let b = Timestamp::new(200, tb_a);
  assert_eq!(a.cmp_semantic(&b), Ordering::Less);
  assert_eq!(b.cmp_semantic(&a), Ordering::Greater);
  assert_eq!(a.cmp_semantic(&a), Ordering::Equal);

  // Cross-timebase slow path: Less / Greater / Equal.
  let one_second_ms = Timestamp::new(1000, tb_a);
  let one_second_mpg = Timestamp::new(90_000, tb_b);
  let half_second_ms = Timestamp::new(500, tb_a);
  let two_seconds_mpg = Timestamp::new(180_000, tb_b);
  assert_eq!(half_second_ms.cmp_semantic(&one_second_mpg), Ordering::Less,);
  assert_eq!(
    two_seconds_mpg.cmp_semantic(&one_second_ms),
    Ordering::Greater,
  );
  assert_eq!(one_second_ms.cmp_semantic(&one_second_mpg), Ordering::Equal,);
}

#[test]
fn saturating_sub_duration_saturates() {
  let tb = Timebase::new(1, nz(1000));
  // Subtracting a finite duration from a small pts shouldn't panic —
  // it saturates at i64::MIN for pathological inputs.
  let near_floor = Timestamp::new(i64::MIN + 10, tb);
  let shifted = near_floor.saturating_sub_duration(Duration::from_secs(1));
  assert_eq!(shifted.pts(), i64::MIN);

  // Normal case: 1500 ms - 500 ms → 1000 ms.
  let ts = Timestamp::new(1500, tb);
  let shifted = ts.saturating_sub_duration(Duration::from_millis(500));
  assert_eq!(shifted.pts(), 1000);
}

#[test]
fn saturating_add_duration_is_the_forward_twin() {
  let tb = Timebase::new(1, nz(1000));

  // Normal case, and the round trip back through the backward twin.
  let ts = Timestamp::new(1500, tb);
  let shifted = ts.saturating_add_duration(Duration::from_millis(500));
  assert_eq!(shifted.pts(), 2000);
  assert_eq!(shifted.timebase(), tb);
  assert_eq!(
    shifted.saturating_sub_duration(Duration::from_millis(500)),
    ts
  );

  // Saturates at the ceiling rather than wrapping.
  let near_ceiling = Timestamp::new(i64::MAX - 10, tb);
  assert_eq!(
    near_ceiling
      .saturating_add_duration(Duration::from_secs(1))
      .pts(),
    i64::MAX
  );

  // A duration too large for the timebase saturates inside
  // `saturating_duration_to_pts`, before the addition ever runs.
  assert_eq!(
    Timestamp::new(0, tb)
      .saturating_add_duration(Duration::MAX)
      .pts(),
    i64::MAX
  );

  // Zero is the identity; a degenerate timebase converts every duration to
  // zero units, so it is the identity there too.
  assert_eq!(ts.saturating_add_duration(Duration::ZERO), ts);
  let degenerate = Timestamp::new(7, Timebase::new(0, nz(3)));
  assert_eq!(
    degenerate
      .saturating_add_duration(Duration::from_secs(1))
      .pts(),
    7
  );
}

#[test]
fn time_range_builders_and_setters() {
  let tb = Timebase::new(1, nz(1000));
  let r = TimeRange::new(0, 0, tb);

  // with_start / with_end — consuming form.
  let r2 = r.with_start(100).with_end(500);
  assert_eq!(r2.start_pts(), 100);
  assert_eq!(r2.end_pts(), 500);

  // set_start / set_end — in-place form, chainable.
  let mut r3 = TimeRange::new(0, 0, tb);
  r3.set_start(10).set_end(20);
  assert_eq!(r3.start_pts(), 10);
  assert_eq!(r3.end_pts(), 20);
}

#[test]
fn time_range_total_pts() {
  let tb = Timebase::new(1, nz(1000));
  assert_eq!(TimeRange::new(100, 500, tb).total_pts(), 400);
  assert_eq!(TimeRange::new(0, 0, tb).total_pts(), 0);
  // Saturating: i64::MIN..i64::MAX would overflow a signed subtract.
  assert_eq!(TimeRange::new(i64::MIN, i64::MAX, tb).total_pts(), i64::MAX);
}

#[test]
fn time_range_rescale_to() {
  let ms = Timebase::new(1, nz(1000));
  let mpeg = Timebase::new(1, nz(90_000));
  let r = TimeRange::new(1000, 2000, ms);
  let r2 = r.rescale_to(mpeg);
  assert_eq!(r2.start_pts(), 90_000);
  assert_eq!(r2.end_pts(), 180_000);
  assert_eq!(r2.timebase(), mpeg);
  // Same span in Duration terms.
  assert_eq!(r.duration(), r2.duration());
  // Instant range stays instant.
  let inst = TimeRange::instant(Timestamp::new(500, ms));
  assert!(inst.rescale_to(mpeg).is_instant());
}

#[test]
fn time_range_try_new() {
  let tb = Timebase::new(1, nz(1000));
  // Forward range: Some.
  let r = TimeRange::try_new(100, 500, tb).unwrap();
  assert_eq!(r.start_pts(), 100);
  assert_eq!(r.end_pts(), 500);
  // Degenerate instant: allowed.
  assert!(TimeRange::try_new(42, 42, tb).is_some());
  // Inverted range: None instead of panic.
  assert!(TimeRange::try_new(500, 100, tb).is_none());
}

#[test]
#[should_panic(expected = "end must not precede start")]
fn time_range_new_panics_on_negative_duration() {
  let tb = Timebase::new(1, nz(1000));
  TimeRange::new(500, 100, tb);
}

#[test]
fn timebase_display_is_num_over_den() {
  // The form proposed in the issue this impl answers, adopted verbatim:
  // https://github.com/findit-studio/mediatime/issues/13
  let timebase = Timebase::new(1, nz(1000));
  assert_eq!(format!("{timebase}"), "1/1000");
  assert_eq!(format!("{timebase:#}"), "1/1000");

  assert_eq!(format!("{}", Timebase::new(1, nz(90_000))), "1/90000");
  assert_eq!(format!("{}", Timebase::new(30_000, nz(1001))), "30000/1001");
  assert_eq!(format!("{}", Timebase::new(0, nz(3))), "0/3");
}

#[test]
fn timebase_display_does_not_reduce() {
  // `2/4 == 1/2` and the two hash alike, but Display shows what the stream
  // declared rather than the canonical form.
  let coarse = Timebase::new(2, nz(4));
  assert_eq!(coarse, Timebase::new(1, nz(2)));
  assert_eq!(format!("{coarse}"), "2/4");
  assert_eq!(format!("{coarse:#}"), "2/4");
}

#[test]
fn timestamp_display_reads_as_a_clock() {
  let mpeg = Timebase::new(1, nz(90_000));
  assert_eq!(format!("{}", Timestamp::new(12_345, mpeg)), "0:00:00.137");
  assert_eq!(
    format!("{:#}", Timestamp::new(12_345, mpeg)),
    "12345 @ 1/90000"
  );

  let ms = Timebase::new(1, nz(1000));
  assert_eq!(format!("{}", Timestamp::new(0, ms)), "0:00:00.000");
  assert_eq!(format!("{:#}", Timestamp::new(0, ms)), "0 @ 1/1000");
  assert_eq!(format!("{}", Timestamp::new(3_661_500, ms)), "1:01:01.500");

  let audio = Timebase::new(1, nz(48_000));
  assert_eq!(format!("{}", Timestamp::new(48_000, audio)), "0:00:01.000");
}

#[test]
fn timestamp_display_truncates_toward_zero() {
  // 44999/90000 s = 0.4999888…; rounding would give .500.
  let mpeg = Timebase::new(1, nz(90_000));
  assert_eq!(format!("{}", Timestamp::new(44_999, mpeg)), "0:00:00.499");
  assert_eq!(format!("{}", Timestamp::new(-44_999, mpeg)), "-0:00:00.499");
}

#[test]
fn timestamp_display_signs_the_whole_rendering() {
  // Negative PTS is ordinary here — pre-roll and edit lists produce it.
  let ms = Timebase::new(1, nz(1000));
  assert_eq!(format!("{}", Timestamp::new(-1500, ms)), "-0:00:01.500");
  assert_eq!(format!("{:#}", Timestamp::new(-1500, ms)), "-1500 @ 1/1000");

  // Under a millisecond the truncated value is zero, and a signed zero would
  // claim a precision this form does not have — so the sign goes with it.
  // `{:#}` still reports which side of zero the PTS was on.
  let mpeg = Timebase::new(1, nz(90_000));
  assert_eq!(format!("{}", Timestamp::new(-1, mpeg)), "0:00:00.000");
  assert_eq!(format!("{:#}", Timestamp::new(-1, mpeg)), "-1 @ 1/90000");
}

#[test]
fn timestamp_display_survives_i64_min() {
  // `i64::MIN` is FFmpeg's `AV_NOPTS_VALUE`, so it reaches this code in
  // practice. Negating it to take a magnitude would overflow; `unsigned_abs`
  // is why this renders instead of panicking.
  let ms = Timebase::new(1, nz(1000));
  let floor = Timestamp::new(i64::MIN, ms);
  assert_eq!(format!("{floor}"), "-2562047788015:12:55.808");
  assert_eq!(format!("{floor:#}"), "-9223372036854775808 @ 1/1000");

  let ceiling = Timestamp::new(i64::MAX, ms);
  assert_eq!(format!("{ceiling}"), "2562047788015:12:55.807");

  let ntsc = Timebase::new(30_000, nz(1001));
  assert_eq!(
    format!("{}", Timestamp::new(i64::MIN, ntsc)),
    "-76784648991465000:03:59.760"
  );

  // The widest intermediate this impl can form: `i64::MIN` against the
  // largest numerator and the smallest denominator. |pts · num · 1000| is
  // 104 bits here — the worst case over the whole input domain — which is
  // what the `i128` promotion buys and what a 25-digit hour field costs.
  let widest = Timebase::new(i32::MAX, nz(1));
  assert_eq!(
    format!("{}", Timestamp::new(i64::MIN, widest)),
    "-5501955727595197878203114:22:56.000"
  );
}

#[test]
fn timestamp_display_with_a_zero_numerator_timebase() {
  // A zero numerator is a legal degenerate timebase (see `timebase_num_zero`)
  // that maps every PTS onto the instant zero. The denominator is `NonZero`,
  // so nothing here divides by zero.
  let degenerate = Timebase::new(0, nz(3));
  assert_eq!(
    format!("{}", Timestamp::new(999_999, degenerate)),
    "0:00:00.000"
  );
  assert_eq!(
    format!("{:#}", Timestamp::new(999_999, degenerate)),
    "999999 @ 0/3"
  );
  assert_eq!(
    format!("{}", Timestamp::new(i64::MIN, degenerate)),
    "0:00:00.000"
  );
}

#[test]
fn timestamp_display_hours_are_unpadded_and_unbounded() {
  // Hours are neither padded to two digits nor wrapped at 24 or 99.
  let ms = Timebase::new(1, nz(1000));
  assert_eq!(
    format!("{}", Timestamp::new(445_506_789, ms)),
    "123:45:06.789"
  );
  assert_eq!(format!("{}", Timestamp::new(9_000_000, ms)), "2:30:00.000");
}

#[test]
fn time_range_display_shows_a_half_open_interval() {
  let ms = Timebase::new(1, nz(1000));
  let range = TimeRange::new(1500, 3250, ms);
  assert_eq!(format!("{range}"), "[0:00:01.500, 0:00:03.250)");
  assert_eq!(format!("{range:#}"), "[1500, 3250) @ 1/1000");

  // The timebase is named once because both endpoints share it.
  let preroll = TimeRange::new(-1500, 3250, ms);
  assert_eq!(format!("{preroll}"), "[-0:00:01.500, 0:00:03.250)");
  assert_eq!(format!("{preroll:#}"), "[-1500, 3250) @ 1/1000");

  let instant = TimeRange::instant(Timestamp::new(12_345, Timebase::new(1, nz(90_000))));
  assert_eq!(format!("{instant}"), "[0:00:00.137, 0:00:00.137)");
  assert_eq!(format!("{instant:#}"), "[12345, 12345) @ 1/90000");
}

#[test]
fn display_ignores_width_and_alignment() {
  // Documented rather than accidental: padding means measuring the finished
  // string, and there is no `alloc` here to build one in. Pinned so that a
  // later change to `f.pad`-style formatting is a deliberate one.
  let ms = Timebase::new(1, nz(1000));
  assert_eq!(format!("{ms:>20}"), "1/1000");
  assert_eq!(
    format!(
      "{:>20}",
      Timestamp::new(12_345, Timebase::new(1, nz(90_000)))
    ),
    "0:00:00.137"
  );
  assert_eq!(
    format!("{:>40}", TimeRange::new(1500, 3250, ms)),
    "[0:00:01.500, 0:00:03.250)"
  );
}

#[test]
fn alternate_display_recovers_what_the_clock_drops() {
  // Two instants a hair apart in different timebases render the same clock;
  // only `{:#}` and `Debug` tell them apart.
  let a = Timestamp::new(12_345, Timebase::new(1, nz(90_000)));
  let b = Timestamp::new(137, Timebase::new(1, nz(1000)));
  assert_eq!(format!("{a}"), format!("{b}"));
  assert_ne!(a, b);
  assert_ne!(format!("{a:#}"), format!("{b:#}"));
  assert_ne!(format!("{a:?}"), format!("{b:?}"));
}
