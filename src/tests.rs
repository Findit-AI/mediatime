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
  assert_eq!(
    Rate::fps(fps.num(), fps.den()).checked_frames_to_duration(24),
    Some(Duration::from_secs(1))
  );

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
fn timestamp_eq_stays_transitive_on_degenerate_timebases() {
  // Every PTS of a `0/den` tick names instant zero, so all three of these are
  // the same instant. The unguarded identical-timebase fast path called the
  // first two unequal while the cross-multiply called each of them equal to
  // the third — an `==` that is not an equivalence, and an `Ord` that is not
  // an order.
  let a = Timestamp::new(1, Timebase::new(0, nz(3)));
  let b = Timestamp::new(2, Timebase::new(0, nz(3)));
  let c = Timestamp::new(1, Timebase::new(0, nz(5)));
  assert_eq!(a, b);
  assert_eq!(b, c);
  assert_eq!(a, c);

  // Instant zero in a timebase that spans time is the same instant again, and
  // `Hash` reduces all four to `(0, 1)` — it agreed with the semantics before
  // the guard did.
  let origin = Timestamp::new(0, Timebase::MILLIS);
  assert_eq!(a, origin);
  assert_eq!(hash_of(&a), hash_of(&b));
  assert_eq!(hash_of(&a), hash_of(&origin));
}

#[test]
fn degenerate_timestamps_are_one_key_in_an_ordered_container() {
  use std::collections::BTreeMap;

  // What an intransitive comparison costs a caller: `BTreeMap` and `sort` both
  // assume `Ord` is a total order, and neither re-checks.
  let a = Timestamp::new(1, Timebase::new(0, nz(3)));
  let b = Timestamp::new(2, Timebase::new(0, nz(3)));
  let c = Timestamp::new(1, Timebase::new(0, nz(5)));

  let mut map = BTreeMap::new();
  map.insert(a, "a");
  map.insert(b, "b");
  map.insert(c, "c");
  assert_eq!(map.len(), 1);
  assert_eq!(map.get(&a), Some(&"c"));
  assert_eq!(map.get(&b), Some(&"c"));
  assert_eq!(map.get(&Timestamp::new(0, Timebase::MILLIS)), Some(&"c"));

  // Sorted means every earlier element is `<=` every later one, not merely its
  // neighbour: the pairwise check is what an intransitive comparator fails.
  let mut sorted = [b, c, a];
  sorted.sort();
  for (i, earlier) in sorted.iter().enumerate() {
    for later in &sorted[i + 1..] {
      assert!(earlier <= later);
    }
  }
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
  let fps30 = Rate::hz(30);
  assert_eq!(
    fps30.checked_frames_to_duration(15),
    Some(Duration::from_millis(500))
  );
  assert_eq!(
    fps30.checked_frames_to_duration(30),
    Some(Duration::from_secs(1))
  );
  assert_eq!(fps30.checked_frames_to_duration(0), Some(Duration::ZERO));
  assert_eq!(
    fps30.saturating_frames_to_duration(15),
    Duration::from_millis(500)
  );
}

#[test]
fn frames_to_duration_ntsc() {
  // 30000 frames @ 30000/1001 fps = exactly 1001 seconds.
  let ntsc = Rate::fps(30_000, nz(1001));
  assert_eq!(
    ntsc.checked_frames_to_duration(30_000),
    Some(Duration::from_secs(1001))
  );
  // 15 frames at NTSC ≈ 500.5 ms.
  assert_eq!(
    ntsc.checked_frames_to_duration(15),
    Some(Duration::from_nanos(500_500_000))
  );
}

#[test]
fn frames_to_duration_refuses_or_clamps_what_no_duration_holds() {
  let fps30 = Rate::hz(30);
  // A negative frame count has no `Duration`; the saturating rung clamps to
  // the floor of the type, as the tick conversion it delegates to does.
  assert_eq!(fps30.checked_frames_to_duration(-1), None);
  assert_eq!(fps30.saturating_frames_to_duration(-1), Duration::ZERO);

  // Past `Duration::MAX`: one event per `i32::MAX` seconds, `i64::MAX` of
  // them.
  let glacial = Rate::fps(1, nz(i32::MAX));
  assert_eq!(glacial.checked_frames_to_duration(i64::MAX), None);
  assert_eq!(
    glacial.saturating_frames_to_duration(i64::MAX),
    Duration::MAX
  );

  // Rounding is the crate's: 1 frame at 3 fps is 333333333.33… ns.
  assert_eq!(
    Rate::hz(3).checked_frames_to_duration(1),
    Some(Duration::from_nanos(333_333_333))
  );
  assert_eq!(
    Rate::hz(3).checked_frames_to_duration(2),
    Some(Duration::from_nanos(666_666_667))
  );
}

#[test]
fn frames_to_duration_and_the_degenerate_rate() {
  // No events per second: no count of them takes any time, not even none of
  // them. The checked rung says so.
  let never = Rate::hz(0);
  assert_eq!(never.checked_frames_to_duration(1), None);
  assert_eq!(never.checked_frames_to_duration(0), None);
  assert_eq!(never.checked_to_timebase(), None);
}

#[test]
#[should_panic(expected = "rate numerator must be non-zero")]
fn saturating_frames_to_duration_panics_on_a_degenerate_rate() {
  Rate::hz(0).saturating_frames_to_duration(1);
}

#[test]
#[should_panic(expected = "rate numerator must be non-zero")]
fn to_timebase_panics_on_a_degenerate_rate() {
  Rate::hz(0).to_timebase();
}

#[test]
#[should_panic(expected = "timebase numerator must be non-zero")]
fn from_timebase_panics_on_a_degenerate_timebase() {
  Rate::from_timebase(Timebase::new(0, nz(3)));
}

#[test]
fn rate_constructors_route_through_the_timebase_gate() {
  assert_eq!(Rate::hz(30).num(), 30);
  assert_eq!(Rate::hz(30).den().get(), 1);
  assert_eq!(Rate::fps(30_000, nz(1001)), Rate::FPS_29_97);
  assert_eq!(Rate::try_hz(30), Some(Rate::hz(30)));
  assert_eq!(Rate::try_fps(30_000, nz(1001)), Some(Rate::FPS_29_97));

  // The degenerate rate is legal to build, as the degenerate timebase is.
  assert_eq!(Rate::try_hz(0), Some(Rate::hz(0)));

  // And the constructor's refusals are the timebase's.
  assert_eq!(Rate::try_hz(-1), None);
  assert_eq!(Rate::try_fps(-1, nz(1001)), None);
  assert_eq!(Rate::try_fps(30, nz(-1)), None);
}

#[test]
#[should_panic(expected = "timebase numerator must not be negative")]
fn rate_hz_panics_on_a_negative_count() {
  Rate::hz(-1);
}

#[test]
fn a_rate_is_a_timebase_read_backwards() {
  // The roster entries are reciprocals of each other, name for name.
  assert_eq!(Rate::FPS_23_976.to_timebase(), Timebase::NTSC_FILM);
  assert_eq!(Rate::FPS_29_97.to_timebase(), Timebase::NTSC_VIDEO);
  assert_eq!(Rate::FPS_24.to_timebase(), Timebase::FILM_24);
  assert_eq!(Rate::FPS_25.to_timebase(), Timebase::PAL_25);
  assert_eq!(Rate::from_timebase(Timebase::FILM_24), Rate::FPS_24);
  assert_eq!(Rate::from_timebase(Timebase::NTSC_VIDEO), Rate::FPS_29_97);

  // Nothing is reduced or normalized on the way there and back.
  let declared = Rate::fps(60_000, nz(2002));
  let there_and_back = Rate::from_timebase(declared.to_timebase());
  assert_eq!(format!("{there_and_back:?}"), format!("{declared:?}"));

  // An audio sample rate is the same reading, and `HZ_48K` is its reciprocal.
  assert_eq!(Rate::hz(48_000).to_timebase(), Timebase::HZ_48K);
}

#[test]
fn rate_equality_and_order_are_the_rationals() {
  // Value-based, as `Timebase`'s are: two spellings of 29.97 are one rate.
  assert_eq!(Rate::fps(60_000, nz(2002)), Rate::FPS_29_97);
  assert_eq!(
    hash_of(&Rate::fps(60_000, nz(2002))),
    hash_of(&Rate::FPS_29_97)
  );
  // A greater rational is a faster rate.
  assert!(Rate::FPS_23_976 < Rate::FPS_24);
  assert!(Rate::FPS_29_97 < Rate::FPS_30);
  assert!(Rate::FPS_59_94 < Rate::FPS_60);
  assert!(Rate::hz(0) < Rate::FPS_23_976);
  // The identity rational, as `Timebase::default` is.
  assert_eq!(Rate::default(), Rate::hz(1));
}

#[test]
fn well_known_rates_read_both_ways() {
  // The table is the single name source, so every entry answers in both
  // directions or in neither.
  for (name, rate) in WELL_KNOWN_RATES {
    assert_eq!(Rate::from_name(name), Some(*rate), "{name}");
    assert_eq!(rate.well_known_name(), Some(*name), "{name}");
  }
  // A count, so adding a constant without listing it here is noticed.
  assert_eq!(WELL_KNOWN_RATES.len(), 8);
}

#[test]
fn well_known_rates_are_pairwise_distinct() {
  // What makes `well_known_name` single-valued.
  for (i, (name, rate)) in WELL_KNOWN_RATES.iter().enumerate() {
    for (other_name, other) in &WELL_KNOWN_RATES[i + 1..] {
      assert_ne!(rate, other, "{name} and {other_name} are the same rational");
    }
  }
}

#[test]
fn well_known_rate_names_do_not_collide_under_ascii_folding() {
  // `from_name` folds ASCII case, so two names differing only in case would
  // make the forward lookup depend on table order.
  for (i, (name, _)) in WELL_KNOWN_RATES.iter().enumerate() {
    for (other_name, _) in &WELL_KNOWN_RATES[i + 1..] {
      assert!(
        !name.eq_ignore_ascii_case(other_name),
        "{name} and {other_name} fold together"
      );
    }
  }
}

#[test]
fn rate_from_name_folds_case_and_nothing_else() {
  // Any casing of the constant's name reads.
  assert_eq!(Rate::from_name("FPS_29_97"), Some(Rate::FPS_29_97));
  assert_eq!(Rate::from_name("fps_29_97"), Some(Rate::FPS_29_97));
  assert_eq!(Rate::from_name("Fps_29_97"), Some(Rate::FPS_29_97));

  // Case is the whole of the folding: no trimming, no separator guessing, no
  // rational parsing.
  assert_eq!(Rate::from_name(" FPS_24"), None);
  assert_eq!(Rate::from_name("FPS 24"), None);
  assert_eq!(Rate::from_name("FPS-24"), None);
  assert_eq!(Rate::from_name("24"), None);
  assert_eq!(Rate::from_name("24/1"), None);
  assert_eq!(Rate::from_name(""), None);

  // And the canonical spelling is what comes back out.
  assert_eq!(
    Rate::from_name("fps_24").and_then(|r| r.well_known_name()),
    Some("FPS_24")
  );
}

#[test]
fn rate_well_known_name_matches_by_value() {
  // As `Timebase::well_known_name` does: a stream that declared `60000/2002`
  // is counting 29.97 and answers to the name.
  assert_eq!(
    Rate::fps(60_000, nz(2002)).well_known_name(),
    Some("FPS_29_97")
  );
  assert_eq!(Rate::hz(48_000).well_known_name(), None);
  assert_eq!(Rate::hz(0).well_known_name(), None);
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
  // for — the trap the roster's doc warns about, and what `Rate` is the other
  // side of.
  assert_eq!(
    Rate::from_timebase(Timebase::NTSC_VIDEO).checked_frames_to_duration(30_000),
    Some(Duration::from_secs(1001))
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
fn from_name_folds_case_and_nothing_else() {
  // Any ASCII casing of the constant's own spelling reads.
  for s in ["MILLIS", "millis", "Millis", "mIlLiS"] {
    assert_eq!(Timebase::from_name(s), Some(Timebase::MILLIS), "{s:?}");
  }

  // Case is the whole of the folding: no alias, no trimming, no rational
  // parsing on this door.
  for s in [" MILLIS", "MILLIS ", "MS", "MILLI", "1/1000", ""] {
    assert_eq!(Timebase::from_name(s), None, "{s:?}");
  }

  // And the canonical spelling is what comes back out.
  assert_eq!(
    Timebase::from_name("millis").and_then(|tb| tb.well_known_name()),
    Some("MILLIS")
  );
}

#[test]
fn well_known_timebase_names_do_not_collide_under_ascii_folding() {
  // `from_name` folds ASCII case, so two names differing only in case would
  // make the forward lookup depend on table order.
  for (i, (name, _)) in WELL_KNOWN.iter().enumerate() {
    for (other_name, _) in &WELL_KNOWN[i + 1..] {
      assert!(
        !name.eq_ignore_ascii_case(other_name),
        "{name} and {other_name} fold together"
      );
    }
  }
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
fn duration_to_pts_refuses_a_degenerate_timebase() {
  // Every tick of a `0/den` timebase is instant zero, so no count of them
  // spans a second — nor a zero duration, the refusal being about the
  // timebase and not about `d`. `checked_rescale` refuses its degenerate
  // target the same way, at `pts = 0` included.
  let degenerate = Timebase::new(0, nz(3));
  assert_eq!(
    degenerate.checked_duration_to_pts(Duration::from_secs(1)),
    None
  );
  assert_eq!(degenerate.checked_duration_to_pts(Duration::ZERO), None);
}

#[test]
#[should_panic(expected = "target timebase numerator must be non-zero")]
fn saturating_duration_to_pts_panics_on_a_degenerate_timebase() {
  // The other rung of the same ladder panics on the same degeneracy, in the
  // same words: saturation is a posture toward overflow, and a degenerate
  // timebase leaves nothing to clamp. It answered `0` before, which was the
  // honest count only for `Duration::ZERO`.
  Timebase::new(0, nz(3)).saturating_duration_to_pts(Duration::from_secs(1));
}

#[test]
#[should_panic(expected = "target timebase numerator must be non-zero")]
fn saturating_duration_to_pts_panics_on_a_degenerate_timebase_for_zero_too() {
  // Not even the duration whose old answer was right: the refusal is about
  // the timebase.
  Timebase::new(0, nz(3)).saturating_duration_to_pts(Duration::ZERO);
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

  // Zero is the identity.
  assert_eq!(ts.saturating_add_duration(Duration::ZERO), ts);
}

#[test]
#[should_panic(expected = "target timebase numerator must be non-zero")]
fn saturating_add_duration_panics_on_a_degenerate_timebase() {
  // It was a no-op here while `saturating_duration_to_pts` answered `0`;
  // that conversion now refuses the degenerate timebase, and the shift
  // built on it inherits the refusal rather than pretending to have moved.
  Timestamp::new(7, Timebase::new(0, nz(3))).saturating_add_duration(Duration::from_secs(1));
}

#[test]
#[should_panic(expected = "target timebase numerator must be non-zero")]
fn saturating_sub_duration_panics_on_a_degenerate_timebase() {
  Timestamp::new(7, Timebase::new(0, nz(3))).saturating_sub_duration(Duration::from_secs(1));
}

#[test]
fn signed_duration_accessors_and_predicates() {
  let ms = Timebase::MILLIS;
  let backwards = SignedDuration::new(-1500, ms);
  assert_eq!(backwards.ticks(), -1500);
  assert_eq!(backwards.timebase(), ms);
  assert!(backwards.is_negative());
  assert!(!backwards.is_positive());
  assert!(!backwards.is_zero());

  let forwards = SignedDuration::new(1500, ms);
  assert!(forwards.is_positive());
  assert!(!forwards.is_negative());

  let still = SignedDuration::new(0, ms);
  assert!(still.is_zero());
  assert!(!still.is_positive());
  assert!(!still.is_negative());

  // The default is the zero span, in the default timebase.
  assert_eq!(SignedDuration::default().ticks(), 0);
  assert_eq!(SignedDuration::default().timebase(), Timebase::default());
}

#[test]
fn signed_duration_neg_and_abs() {
  let ms = Timebase::MILLIS;
  let backwards = SignedDuration::new(-1500, ms);
  assert_eq!(backwards.checked_neg(), Some(SignedDuration::new(1500, ms)));
  assert_eq!(backwards.saturating_neg(), SignedDuration::new(1500, ms));
  assert_eq!(backwards.checked_abs(), Some(SignedDuration::new(1500, ms)));
  assert_eq!(backwards.saturating_abs(), SignedDuration::new(1500, ms));

  // Both keep the timebase; only the count moves.
  assert_eq!(backwards.checked_neg().unwrap().timebase(), ms);

  // `abs` differs from `neg` on a forward span: it is the identity.
  let forwards = SignedDuration::new(1500, ms);
  assert_eq!(forwards.checked_abs(), Some(forwards));
  assert_eq!(forwards.checked_neg(), Some(backwards));
}

#[test]
fn signed_duration_neg_and_abs_at_the_floor() {
  // `i64::MIN` has no positive twin, which is the one input where the two
  // rungs part: the checked one refuses, the saturating one clamps.
  let floor = SignedDuration::new(i64::MIN, Timebase::MILLIS);
  assert_eq!(floor.checked_neg(), None);
  assert_eq!(floor.checked_abs(), None);
  assert_eq!(floor.saturating_neg().ticks(), i64::MAX);
  assert_eq!(floor.saturating_abs().ticks(), i64::MAX);
}

#[test]
fn signed_duration_add_and_sub_in_one_timebase_are_exact() {
  let ms = Timebase::MILLIS;
  let a = SignedDuration::new(1500, ms);
  let b = SignedDuration::new(-500, ms);
  assert_eq!(a.checked_add(b), Some(SignedDuration::new(1000, ms)));
  assert_eq!(a.saturating_add(b), SignedDuration::new(1000, ms));
  assert_eq!(a.checked_sub(b), Some(SignedDuration::new(2000, ms)));
  assert_eq!(a.saturating_sub(b), SignedDuration::new(2000, ms));

  // Addition and subtraction undo each other exactly here.
  assert_eq!(a.checked_add(b).unwrap().checked_sub(b), Some(a));
}

#[test]
fn signed_duration_arithmetic_answers_in_the_left_timebase() {
  let ms = Timebase::MILLIS;
  let mpeg = Timebase::MPEG_90K;

  // One second either way, counted on whichever clock is on the left.
  let in_ms = SignedDuration::new(1000, ms)
    .checked_add(SignedDuration::new(90_000, mpeg))
    .expect("both spans fit");
  assert_eq!(in_ms, SignedDuration::new(2000, ms));

  let in_mpeg = SignedDuration::new(90_000, mpeg)
    .checked_add(SignedDuration::new(1000, ms))
    .expect("both spans fit");
  assert_eq!(in_mpeg, SignedDuration::new(180_000, mpeg));

  // The same span, at two resolutions.
  assert!(in_ms.cmp_semantic(&in_mpeg).is_eq());

  // A coarse left operand rounds the finer right one, to nearest and away
  // from zero: half a tick of 1/3 s lands on the far side of the tie.
  let thirds = Timebase::new(1, nz(3));
  let zero = SignedDuration::new(0, thirds);
  assert_eq!(
    zero.checked_add(SignedDuration::new(500, ms)),
    Some(SignedDuration::new(2, thirds))
  );
  assert_eq!(
    zero.checked_add(SignedDuration::new(-500, ms)),
    Some(SignedDuration::new(-2, thirds))
  );
}

#[test]
fn signed_duration_arithmetic_saturates_where_the_checked_rung_refuses() {
  let ms = Timebase::MILLIS;
  let ceiling = SignedDuration::new(i64::MAX, ms);
  let floor = SignedDuration::new(i64::MIN, ms);
  let one = SignedDuration::new(1, ms);

  assert_eq!(ceiling.checked_add(one), None);
  assert_eq!(ceiling.saturating_add(one), ceiling);
  assert_eq!(floor.checked_sub(one), None);
  assert_eq!(floor.saturating_sub(one), floor);

  // The rescale can refuse before the addition does: `i32::MAX` seconds per
  // tick into `1/i32::MAX` seconds per tick is far past `i64`.
  let coarse = SignedDuration::new(1_000_000, Timebase::new(i32::MAX, nz(1)));
  let fine = SignedDuration::new(0, Timebase::new(1, nz(i32::MAX)));
  assert_eq!(fine.checked_add(coarse), None);
  assert_eq!(fine.saturating_add(coarse).ticks(), i64::MAX);
}

#[test]
fn signed_duration_arithmetic_and_the_degenerate_timebase() {
  // Two spans counted in one degenerate timebase add without a conversion,
  // so there is nothing to refuse: tick plus tick is exact.
  let degenerate = Timebase::new(0, nz(3));
  let a = SignedDuration::new(5, degenerate);
  let b = SignedDuration::new(2, degenerate);
  assert_eq!(a.checked_add(b), Some(SignedDuration::new(7, degenerate)));
  assert_eq!(a.saturating_add(b), SignedDuration::new(7, degenerate));

  // A *differing* timebase needs the rescale that a degenerate target
  // refuses — even another degenerate one.
  let elsewhere = SignedDuration::new(2, Timebase::new(0, nz(5)));
  assert_eq!(a.checked_add(elsewhere), None);
  assert_eq!(a.checked_sub(elsewhere), None);
  assert_eq!(
    a.checked_add(SignedDuration::new(2, Timebase::MILLIS)),
    None
  );
}

#[test]
#[should_panic(expected = "target timebase numerator must be non-zero")]
fn signed_duration_saturating_add_panics_on_a_degenerate_left_timebase() {
  let degenerate = SignedDuration::new(5, Timebase::new(0, nz(3)));
  degenerate.saturating_add(SignedDuration::new(2, Timebase::MILLIS));
}

#[test]
fn signed_duration_rescale_to_and_its_checked_rung() {
  let ms = Timebase::MILLIS;
  let mpeg = Timebase::MPEG_90K;
  let backwards = SignedDuration::new(-1000, ms);
  assert_eq!(
    backwards.rescale_to(mpeg),
    SignedDuration::new(-90_000, mpeg)
  );
  assert_eq!(
    backwards.checked_rescale_to(mpeg),
    Some(SignedDuration::new(-90_000, mpeg))
  );

  // The checked rung refuses what the bare one clamps or panics on.
  let coarse = SignedDuration::new(1_000_000, Timebase::new(i32::MAX, nz(1)));
  let fine = Timebase::new(1, nz(i32::MAX));
  assert_eq!(coarse.checked_rescale_to(fine), None);
  assert_eq!(coarse.rescale_to(fine).ticks(), i64::MAX);
  assert_eq!(backwards.checked_rescale_to(Timebase::new(0, nz(3))), None);
}

#[test]
#[should_panic(expected = "target timebase numerator must be non-zero")]
fn signed_duration_rescale_to_panics_on_a_degenerate_target() {
  SignedDuration::new(-1000, Timebase::MILLIS).rescale_to(Timebase::new(0, nz(3)));
}

#[test]
fn signed_duration_equality_is_structural_and_cmp_semantic_is_not() {
  let one_second = SignedDuration::new(1, Timebase::SECONDS);
  let one_thousand_ms = SignedDuration::new(1_000, Timebase::MILLIS);

  // The same span, counted differently: unequal, and semantically equal.
  assert_ne!(one_second, one_thousand_ms);
  assert!(one_second.cmp_semantic(&one_thousand_ms).is_eq());

  // Only the timebase is compared by value, as `Timebase`'s own `==` does —
  // and `Hash` follows that equality.
  let declared = SignedDuration::new(1_000, Timebase::new(2, nz(2000)));
  assert_eq!(one_thousand_ms, declared);
  assert_eq!(hash_of(&one_thousand_ms), hash_of(&declared));
}

#[test]
fn spans_sort_by_length_only_when_asked_to() {
  // Two spellings of one second, a longer span and a shorter one, deliberately
  // mixed: the count alone puts `2 @ 1/1` below `1000 @ 1/1000`, which is the
  // order a derived `Ord` would have handed out and the reason there is none.
  let mut spans = [
    SignedDuration::new(2, Timebase::SECONDS),
    SignedDuration::new(1_000, Timebase::MILLIS),
    SignedDuration::new(-1, Timebase::SECONDS),
    SignedDuration::new(1, Timebase::SECONDS),
    SignedDuration::new(500, Timebase::MILLIS),
  ];
  spans.sort_by(SignedDuration::cmp_semantic);

  // -1s, 500ms, then the two one-second spans in the order they were written
  // (`sort_by` is stable and calls them equal), then 2s.
  assert_eq!(spans[0].ticks(), -1);
  assert_eq!(spans[1].ticks(), 500);
  assert_eq!(spans[2], SignedDuration::new(1_000, Timebase::MILLIS));
  assert_eq!(spans[3], SignedDuration::new(1, Timebase::SECONDS));
  assert_eq!(spans[4].ticks(), 2);

  for (i, shorter) in spans.iter().enumerate() {
    for longer in &spans[i + 1..] {
      assert!(shorter.cmp_semantic(longer).is_le());
    }
  }
}

#[test]
fn signed_duration_cmp_semantic_orders_by_measured_span() {
  let ms = Timebase::MILLIS;
  let mpeg = Timebase::MPEG_90K;
  assert_eq!(
    SignedDuration::new(-1, ms).cmp_semantic(&SignedDuration::new(1, ms)),
    Ordering::Less
  );
  assert_eq!(
    SignedDuration::new(-90_000, mpeg).cmp_semantic(&SignedDuration::new(-1000, ms)),
    Ordering::Equal
  );
  assert_eq!(
    SignedDuration::new(-90_001, mpeg).cmp_semantic(&SignedDuration::new(-1000, ms)),
    Ordering::Less
  );
  assert_eq!(
    SignedDuration::new(500, ms).cmp_semantic(&SignedDuration::new(90_000, mpeg)),
    Ordering::Less
  );
}

#[test]
fn signed_duration_cmp_semantic_stays_transitive_on_degenerate_timebases() {
  // Every count of a `0/den` tick measures zero, so all three of these are
  // the same span. The identical-timebase fast path would have called the
  // first two unequal while the cross-multiply called each of them equal to
  // the third — an order that is not one.
  let a = SignedDuration::new(1, Timebase::new(0, nz(3)));
  let b = SignedDuration::new(2, Timebase::new(0, nz(3)));
  let c = SignedDuration::new(1, Timebase::new(0, nz(5)));
  assert!(a.cmp_semantic(&b).is_eq());
  assert!(b.cmp_semantic(&c).is_eq());
  assert!(a.cmp_semantic(&c).is_eq());
}

#[test]
fn timestamp_signed_duration_since_signs_the_difference() {
  let ms = Timebase::MILLIS;
  let later = Timestamp::new(1500, ms);
  let earlier = Timestamp::new(500, ms);
  assert_eq!(
    later.signed_duration_since(&earlier),
    SignedDuration::new(1000, ms)
  );
  assert_eq!(
    earlier.signed_duration_since(&later),
    SignedDuration::new(-1000, ms)
  );
  assert_eq!(
    later.checked_signed_duration_since(&later),
    Some(SignedDuration::new(0, ms))
  );

  // `duration_since` is the same difference through the unsigned type, and
  // refuses the direction this one reports.
  assert_eq!(earlier.duration_since(&later), None);
}

#[test]
fn timestamp_signed_duration_since_counts_in_the_receiver_timebase() {
  let ms = Timebase::MILLIS;
  let mpeg = Timebase::MPEG_90K;
  let on_mpeg = Timestamp::new(90_000, mpeg);
  let on_ms = Timestamp::new(500, ms);
  assert_eq!(
    on_mpeg.signed_duration_since(&on_ms),
    SignedDuration::new(45_000, mpeg)
  );
  assert_eq!(
    on_ms.signed_duration_since(&on_mpeg),
    SignedDuration::new(-500, ms)
  );
}

#[test]
fn timestamp_signed_duration_since_saturates_where_the_checked_rung_refuses() {
  let ms = Timebase::MILLIS;
  let ceiling = Timestamp::new(i64::MAX, ms);
  let below_zero = Timestamp::new(-1, ms);
  assert_eq!(ceiling.checked_signed_duration_since(&below_zero), None);
  assert_eq!(ceiling.signed_duration_since(&below_zero).ticks(), i64::MAX);
}

#[test]
fn timestamp_shifts_by_a_signed_span() {
  let ms = Timebase::MILLIS;
  let mpeg = Timebase::MPEG_90K;
  let ts = Timestamp::new(1000, ms);

  assert_eq!(
    ts.checked_add_signed(SignedDuration::new(-1500, ms)),
    Some(Timestamp::new(-500, ms))
  );
  assert_eq!(
    ts.saturating_add_signed(SignedDuration::new(-1500, ms)),
    Timestamp::new(-500, ms)
  );
  assert_eq!(
    ts.checked_sub_signed(SignedDuration::new(1500, ms)),
    Some(Timestamp::new(-500, ms))
  );
  assert_eq!(
    ts.saturating_sub_signed(SignedDuration::new(1500, ms)),
    Timestamp::new(-500, ms)
  );

  // A span counted on another clock is rescaled into this one first, and the
  // answer stays in this one.
  let shifted = ts
    .checked_add_signed(SignedDuration::new(90_000, mpeg))
    .expect("one second fits");
  assert_eq!(shifted.pts(), 2000);
  assert_eq!(shifted.timebase(), ms);

  // Shifting by a span and asking for it back returns it.
  let span = SignedDuration::new(-333, ms);
  assert_eq!(
    ts.checked_add_signed(span)
      .unwrap()
      .signed_duration_since(&ts),
    span
  );
}

#[test]
fn timestamp_shifts_saturate_where_the_checked_rung_refuses() {
  let ms = Timebase::MILLIS;
  let ceiling = Timestamp::new(i64::MAX, ms);
  let one = SignedDuration::new(1, ms);
  assert_eq!(ceiling.checked_add_signed(one), None);
  assert_eq!(ceiling.saturating_add_signed(one), ceiling);

  let floor = Timestamp::new(i64::MIN, ms);
  assert_eq!(floor.checked_sub_signed(one), None);
  assert_eq!(floor.saturating_sub_signed(one), floor);

  // Subtracting the most negative span is reachable where negating it is
  // not, which is why the two directions are separate methods.
  assert_eq!(
    Timestamp::new(-1, ms).checked_sub_signed(SignedDuration::new(i64::MIN, ms)),
    Some(Timestamp::new(i64::MAX, ms))
  );
}

#[test]
#[should_panic(expected = "target timebase numerator must be non-zero")]
fn timestamp_saturating_add_signed_panics_on_a_degenerate_timebase() {
  Timestamp::new(7, Timebase::new(0, nz(3)))
    .saturating_add_signed(SignedDuration::new(1, Timebase::MILLIS));
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
