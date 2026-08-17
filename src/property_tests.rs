//! Randomized properties of the rescale ladder and the canonical form.
//!
//! The table-driven tests next door pick their inputs; these let `quickcheck`
//! pick, over the whole `(pts, num, den)` domain the constructors admit —
//! full-range draws, with the type's boundary values salted in. They are not
//! feature-gated: `quickcheck` is an unconditional dev-dependency, and the
//! generators here fold raw `u32`/`i64` draws into valid timebases rather than
//! going through the optional `Arbitrary` impls, so the properties run in
//! every feature configuration.

use super::*;

use quickcheck::{TestResult, quickcheck};

/// Any `Timebase`, the degenerate `0/den` included, folded from an arbitrary
/// `(num, den)` draw.
///
/// Folded rather than rejected because `quickcheck` implements `Arbitrary`
/// only for the unsigned `NonZero` types, so a denominator cannot be drawn at
/// its field type; folding also keeps every draw usable instead of spending
/// the test budget on discards.
fn any_timebase((num, den): (u32, u32)) -> Timebase {
  const MAX: u32 = i32::MAX as u32;
  Timebase::new((num % (MAX + 1)) as i32, nz((den % MAX + 1) as i32))
}

/// A `Timebase` a rescale can target: the same fold with the numerator forced
/// into `1..=i32::MAX`, a zero one being the ladder's refusal arm rather than
/// a quotient with anything to say.
fn target_timebase((num, den): (u32, u32)) -> Timebase {
  const MAX: u32 = i32::MAX as u32;
  Timebase::new((num % MAX + 1) as i32, nz((den % MAX + 1) as i32))
}

/// A `Timebase` drawn from a deliberately tiny pool: numerator in `0..3`,
/// denominator in `1..4`.
///
/// The degenerate `0/den` — and two different spellings of it — come up often
/// here and approximately never in a full-range draw, which is where the
/// comparison laws are hardest and where an identical-timebase fast path can
/// disagree with the cross-multiply.
fn coarse_timebase((num, den): (u32, u32)) -> Timebase {
  Timebase::new((num % 3) as i32, nz((den % 3 + 1) as i32))
}

/// The exact, unrounded quotient of a rescale as `(numerator, denominator)` —
/// rebuilt here from the definition rather than borrowed from the
/// implementation, so a property comparing against it tests the rounding
/// instead of agreeing with itself.
fn exact_quotient(pts: i64, from: Timebase, to: Timebase) -> (i128, i128) {
  (
    (pts as i128) * (from.num() as i128) * (to.den().get() as i128),
    (from.den().get() as i128) * (to.num() as i128),
  )
}

fn hash_of(tb: &Timebase) -> u64 {
  let mut h = std::collections::hash_map::DefaultHasher::new();
  tb.hash(&mut h);
  h.finish()
}

quickcheck! {
  /// Rescaling into the timebase a PTS is already counted in returns it
  /// unchanged — for every legal target and every `i64`.
  fn rescale_into_the_same_timebase_is_the_identity(pts: i64, tb: (u32, u32)) -> bool {
    let tb = target_timebase(tb);
    tb.checked_rescale(pts, tb) == Some(pts) && tb.saturating_rescale(pts, tb) == pts
  }

  /// The tick returned is a *nearest* one: the exact instant is never more
  /// than half a tick away from it.
  fn rescale_lands_within_half_a_tick(pts: i64, from: (u32, u32), to: (u32, u32)) -> TestResult {
    let from = any_timebase(from);
    let to = target_timebase(to);
    // Saturation, not rounding, decides an out-of-range quotient, and a
    // saturated answer is deliberately not a nearest tick.
    let Some(q) = from.checked_rescale(pts, to) else {
      return TestResult::discard();
    };
    let (n, d) = exact_quotient(pts, from, to);
    TestResult::from_bool(2 * (n - (q as i128) * d).abs() <= d)
  }

  /// And when the exact instant falls *exactly* between two ticks, the one
  /// chosen is the one further from zero — FFmpeg's `AV_ROUND_NEAR_INF`.
  ///
  /// The tie is constructed rather than waited for: an odd count of
  /// half-second ticks is a half-integer number of seconds for every odd
  /// `pts`, where random inputs would produce an exact tie approximately
  /// never.
  fn rescale_breaks_ties_away_from_zero(pts: i64) -> bool {
    let half_seconds = Timebase::new(1, nz(2));
    // `| 1` rather than `* 2 + 1`: it cannot overflow at `i64::MIN`, and it
    // is odd at both ends of the range.
    let pts = pts | 1;
    let away = ((pts as i128) + (pts.signum() as i128)) / 2;
    half_seconds.checked_rescale(pts, Timebase::SECONDS) == Some(away as i64)
  }

  /// Rescaling is monotone, so it agrees with `cmp_semantic`: two instants
  /// rescaled into one timebase never come back in the opposite order.
  ///
  /// This is the property `TimeRange::rescale_to` leans on to preserve
  /// `start <= end`, and `Timestamp::rescale_to` to stay a *rescale* rather
  /// than a reshuffle. Rounding can collapse a strict order into equality —
  /// two instants inside one tick of the target — which is why the conclusion
  /// is `<=` rather than `<`.
  ///
  /// The PTS values are drawn as `i8`s on purpose. Two independent `i64`
  /// draws are a decade apart in the target and satisfy this trivially, which
  /// makes for a property that passes a rounding rule that inverts order
  /// (measured: it did). Small values in unrelated timebases land near each
  /// other and astride zero, which is where a rounding discontinuity is
  /// visible.
  fn rescale_preserves_semantic_order(a: (i8, u32, u32), b: (i8, u32, u32), to: (u32, u32)) -> bool {
    let x = Timestamp::new(a.0 as i64, any_timebase((a.1, a.2)));
    let y = Timestamp::new(b.0 as i64, any_timebase((b.1, b.2)));
    let to = target_timebase(to);
    let (rx, ry) = (x.rescale_to(to).pts(), y.rescale_to(to).pts());
    match x.cmp_semantic(&y) {
      Ordering::Less => rx <= ry,
      Ordering::Greater => rx >= ry,
      Ordering::Equal => rx == ry,
    }
  }

  /// `Duration` → ticks is the same conversion as a rescale out of
  /// `Timebase::NANOS`, rounding and refusals included — two spellings of one
  /// operation, which is what makes `NANOS` the timebase a `Duration` is
  /// counted in.
  ///
  /// The duration is drawn small enough for its nanosecond count to be an
  /// `i64`, the one thing a rescale needs that a `Duration` does not carry.
  fn duration_to_pts_is_a_rescale_out_of_nanos(secs: u32, nanos: u32, tb: (u32, u32)) -> bool {
    let d = Duration::new(secs as u64, nanos % 1_000_000_000);
    let tb = any_timebase(tb);
    tb.checked_duration_to_pts(d) == Timebase::NANOS.checked_rescale(d.as_nanos() as i64, tb)
  }

  /// Ticks → `Duration` inverts `Duration` → ticks exactly whenever a tick is
  /// a whole number of nanoseconds — the case every roster timebase down to
  /// `NANOS` is in.
  fn pts_to_duration_inverts_on_whole_nanosecond_ticks(pts: u32, which: usize) -> bool {
    const WHOLE_NANOSECOND_TICKS: &[Timebase] = &[
      Timebase::SECONDS,
      Timebase::MILLIS,
      Timebase::MICROS,
      Timebase::NANOS,
      Timebase::FILM_24,
      Timebase::PAL_25,
      Timebase::HZ_48K,
    ];
    let tb = WHOLE_NANOSECOND_TICKS[which % WHOLE_NANOSECOND_TICKS.len()];
    let pts = pts as i64;
    tb.checked_pts_to_duration(pts).and_then(|d| tb.checked_duration_to_pts(d)) == Some(pts)
  }

  /// Each ladder's two rungs agree wherever the `checked_` one has an answer:
  /// they differ in what they do at the edge, never in the arithmetic.
  fn the_rescale_rungs_agree(pts: i64, from: (u32, u32), to: (u32, u32)) -> bool {
    let (from, to) = (any_timebase(from), target_timebase(to));
    match from.checked_rescale(pts, to) {
      Some(q) => from.saturating_rescale(pts, to) == q,
      None => true,
    }
  }

  /// The `None` arm here must stay an arm and not become a call: `None`
  /// includes the degenerate timebase, where the saturating rung panics.
  fn the_duration_to_pts_rungs_agree(secs: u32, nanos: u32, tb: (u32, u32)) -> bool {
    let d = Duration::new(secs as u64, nanos % 1_000_000_000);
    let tb = any_timebase(tb);
    match tb.checked_duration_to_pts(d) {
      Some(q) => tb.saturating_duration_to_pts(d) == q,
      None => true,
    }
  }

  fn the_pts_to_duration_rungs_agree(pts: i64, tb: (u32, u32)) -> bool {
    let tb = any_timebase(tb);
    match tb.checked_pts_to_duration(pts) {
      Some(q) => tb.saturating_pts_to_duration(pts) == q,
      None => true,
    }
  }

  /// `reduce` is a canonicalization: it keeps the value, lands in lowest
  /// terms, is idempotent, and agrees with the hash — the law that makes it
  /// safe for `Hash` to call it.
  fn reduce_canonicalizes_without_moving_the_value(tb: (u32, u32)) -> bool {
    let tb = any_timebase(tb);
    let reduced = tb.reduce();
    reduced == tb
      && reduced.is_reduced()
      && format!("{:?}", reduced.reduce()) == format!("{reduced:?}")
      && hash_of(&reduced) == hash_of(&tb)
  }

  /// A timebase that answers to a roster name parses back from that name.
  fn the_name_table_reads_both_ways(tb: (u32, u32)) -> bool {
    let tb = any_timebase(tb);
    match tb.well_known_name() {
      Some(name) => Timebase::from_name(name) == Some(tb),
      None => true,
    }
  }

  /// The reciprocal of a reciprocal is where it started — *structurally*, not
  /// merely by value — and the degenerate timebase is the only input without
  /// one.
  fn checked_recip_is_its_own_inverse(tb: (u32, u32)) -> bool {
    let tb = any_timebase(tb);
    match tb.checked_recip().and_then(Timebase::checked_recip) {
      Some(back) => format!("{back:?}") == format!("{tb:?}"),
      None => tb.num() == 0,
    }
  }

  /// A span negated twice is the span it started from, and the timebase does
  /// not move. `i64::MIN` ticks is the one span with no opposite.
  fn negating_a_span_twice_returns_it(ticks: i64, tb: (u32, u32)) -> bool {
    let span = SignedDuration::new(ticks, any_timebase(tb));
    match span.checked_neg().and_then(SignedDuration::checked_neg) {
      Some(back) => back == span,
      None => ticks == i64::MIN,
    }
  }

  /// `abs` is the magnitude: never backwards, either the span or its
  /// negation, and already settled after one application.
  fn abs_is_the_magnitude_of_a_span(ticks: i64, tb: (u32, u32)) -> bool {
    let span = SignedDuration::new(ticks, any_timebase(tb));
    match span.checked_abs() {
      Some(magnitude) => {
        !magnitude.is_negative()
          && (magnitude == span || Some(magnitude) == span.checked_neg())
          && magnitude.checked_abs() == Some(magnitude)
      }
      None => ticks == i64::MIN,
    }
  }

  /// Adding a span and subtracting the same one returns what it started from
  /// — exactly, both spans being counted in one timebase.
  fn adding_a_span_and_subtracting_it_returns_the_first(a: i64, b: i64, tb: (u32, u32)) -> bool {
    let tb = any_timebase(tb);
    let (x, y) = (SignedDuration::new(a, tb), SignedDuration::new(b, tb));
    match x.checked_add(y) {
      Some(sum) => sum.checked_sub(y) == Some(x),
      None => true,
    }
  }

  /// In one timebase the sum of two spans is exactly the sum of two `i64`s:
  /// the counts are added, not converted, so neither rung can round.
  fn spans_in_one_timebase_add_as_i64s(a: i64, b: i64, tb: (u32, u32)) -> bool {
    let tb = any_timebase(tb);
    let (x, y) = (SignedDuration::new(a, tb), SignedDuration::new(b, tb));
    x.checked_add(y).map(|sum| sum.ticks()) == a.checked_add(b)
      && x.saturating_add(y).ticks() == a.saturating_add(b)
      && x.checked_sub(y).map(|d| d.ticks()) == a.checked_sub(b)
      && x.saturating_sub(y).ticks() == a.saturating_sub(b)
  }

  /// Rescaling spans is monotone, so it agrees with `cmp_semantic` — the law
  /// the instant twin obeys, for the reason
  /// `rescale_preserves_semantic_order` gives, including why the counts are
  /// drawn as `i8`s.
  fn rescaling_spans_preserves_semantic_order(a: (i8, u32, u32), b: (i8, u32, u32), to: (u32, u32)) -> bool {
    let x = SignedDuration::new(a.0 as i64, any_timebase((a.1, a.2)));
    let y = SignedDuration::new(b.0 as i64, any_timebase((b.1, b.2)));
    let to = target_timebase(to);
    let (rx, ry) = (x.rescale_to(to).ticks(), y.rescale_to(to).ticks());
    match x.cmp_semantic(&y) {
      Ordering::Less => rx <= ry,
      Ordering::Greater => rx >= ry,
      Ordering::Equal => rx == ry,
    }
  }

  /// `cmp_semantic` is an order, degenerate timebases included — the case
  /// `coarse_timebase` exists to reach, and the one where comparing counts
  /// under an identical-timebase fast path would report an order the spans
  /// do not have.
  fn span_semantic_order_is_transitive(a: (i8, u32, u32), b: (i8, u32, u32), c: (i8, u32, u32)) -> bool {
    let span =
      |(t, num, den): (i8, u32, u32)| SignedDuration::new(t as i64, coarse_timebase((num, den)));
    let (x, y, z) = (span(a), span(b), span(c));
    !(x.cmp_semantic(&y).is_le() && y.cmp_semantic(&z).is_le()) || x.cmp_semantic(&z).is_le()
  }

  /// Every count of a degenerate `0/den` tick measures zero seconds, so all
  /// such spans compare equal — to each other, however each is written, and
  /// to a zero span anywhere else.
  ///
  /// Degenerate **by construction** rather than waited for: a full-range
  /// numerator is zero approximately never, and the transitivity property
  /// above misses this corner about a third of the time (measured against a
  /// fast path with the degeneracy guard removed: 2 failures in 3 runs, where
  /// this property failed 3 in 3). This is where such a fast path reports an
  /// order the spans do not have.
  fn every_span_in_a_degenerate_timebase_measures_zero(a: (i64, u32), b: (i64, u32), tb: (u32, u32)) -> bool {
    let nowhere = |(ticks, den): (i64, u32)| SignedDuration::new(ticks, Timebase::new(0, nz((den % 4 + 1) as i32)));
    let (x, y) = (nowhere(a), nowhere(b));
    x.cmp_semantic(&y).is_eq() && x.cmp_semantic(&SignedDuration::new(0, any_timebase(tb))).is_eq()
  }

  /// Shifting an instant by a span and asking what span separates the two
  /// returns the span — the law that makes the pair inverses.
  fn a_shift_and_the_span_it_moved_by_are_inverses(pts: i64, ticks: i64, tb: (u32, u32)) -> bool {
    let tb = any_timebase(tb);
    let ts = Timestamp::new(pts, tb);
    let span = SignedDuration::new(ticks, tb);
    match ts.checked_add_signed(span) {
      Some(shifted) => shifted.checked_signed_duration_since(&ts) == Some(span),
      None => true,
    }
  }
}
