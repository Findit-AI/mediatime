#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, allow(unused_attributes))]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

use core::{
  cmp::Ordering,
  hash::{Hash, Hasher},
  num::NonZeroU32,
  time::Duration,
};

/// A media timebase represented as a rational number: numerator over non-zero denominator.
///
/// Typical values: `1/1000` for millisecond PTS, `1/90000` for MPEG-TS,
/// `1/48000` for audio samples, `30000/1001` for NTSC video (when used as a
/// frame rate).
///
/// # Equality and ordering
///
/// Comparison is **value-based**: `1/2` equals `2/4`, and `1/3 < 2/3 < 1/1`.
/// [`Hash`] hashes the reduced (lowest-terms) form, so equal rationals hash
/// the same. Cross-multiplication uses `u64` intermediates — exact for any
/// `u32` numerator / denominator.
#[derive(Debug, Clone, Copy, Eq)]
pub struct Timebase {
  num: u32,
  den: NonZeroU32,
}

impl Timebase {
  /// Creates a new `Timebase` with the given numerator and non-zero denominator.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn new(num: u32, den: NonZeroU32) -> Self {
    Self { num, den }
  }

  /// Returns the numerator.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn num(&self) -> u32 {
    self.num
  }

  /// Returns the denominator.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn den(&self) -> NonZeroU32 {
    self.den
  }

  /// Set the value of the numerator.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn with_num(mut self, num: u32) -> Self {
    self.set_num(num);
    self
  }

  /// Set the value of the denominator.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn with_den(mut self, den: NonZeroU32) -> Self {
    self.set_den(den);
    self
  }

  /// Set the value of the numerator in place.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn set_num(&mut self, num: u32) -> &mut Self {
    self.num = num;
    self
  }

  /// Set the value of the denominator in place.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn set_den(&mut self, den: NonZeroU32) -> &mut Self {
    self.den = den;
    self
  }

  /// Rescales `pts` from timebase `from` to timebase `to`, rounding toward zero.
  ///
  /// Equivalent to FFmpeg's `av_rescale_q`. Uses a 128-bit intermediate to
  /// avoid overflow for typical video PTS ranges. If the rescaled value
  /// exceeds `i64`'s range (pathological for real video), the result is
  /// **saturated** to `i64::MIN` or `i64::MAX` — this matches the behavior
  /// promised by `duration_to_pts` and avoids silent wraparound.
  ///
  /// # Panics
  ///
  /// Panics if `to.num() == 0` (division by zero).
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn rescale_pts(pts: i64, from: Self, to: Self) -> i64 {
    assert!(to.num != 0, "target timebase numerator must be non-zero");
    // pts * (from.num / from.den) / (to.num / to.den)
    // = pts * from.num * to.den / (from.den * to.num)
    let numerator = (pts as i128) * (from.num as i128) * (to.den.get() as i128);
    let denominator = (from.den.get() as i128) * (to.num as i128);
    let q = numerator / denominator;
    if q > i64::MAX as i128 {
      i64::MAX
    } else if q < i64::MIN as i128 {
      i64::MIN
    } else {
      q as i64
    }
  }

  /// Rescales `pts` from this timebase to `to`, rounding toward zero.
  ///
  /// Method form of [`Self::rescale_pts`]: `self` is the source timebase.
  ///
  /// # Panics
  ///
  /// Panics if `to.num() == 0` (division by zero).
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn rescale(&self, pts: i64, to: Self) -> i64 {
    Self::rescale_pts(pts, *self, to)
  }

  /// Treats `self` as a frame rate (frames per second) and returns the
  /// [`Duration`] corresponding to `frames` frames.
  ///
  /// Examples:
  /// - 30 fps: `Timebase::new(30, nz(1)).frames_to_duration(15)` → 500 ms
  /// - NTSC: `Timebase::new(30000, nz(1001)).frames_to_duration(30000)` → 1001 ms
  ///
  /// Note that "frame rate" and "PTS timebase" are conceptually *different*
  /// rationals even though both are represented as [`Timebase`]. A 30 fps
  /// stream typically has PTS timebase `1/30` (seconds per unit) and frame
  /// rate `30/1` (frames per second) — they are reciprocals.
  ///
  /// # Panics
  ///
  /// Panics if `self.num() == 0` (division by zero).
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn frames_to_duration(&self, frames: u32) -> Duration {
    // frames / (num/den) seconds = frames * den / num seconds
    let num = self.num as u128;
    let den = self.den.get() as u128;
    assert!(num != 0, "frame rate numerator must be non-zero");
    let total_ns = (frames as u128) * den * 1_000_000_000 / num;
    let secs = (total_ns / 1_000_000_000) as u64;
    let nanos = (total_ns % 1_000_000_000) as u32;
    Duration::new(secs, nanos)
  }

  /// Converts a [`Duration`] into the number of PTS units this timebase
  /// represents, rounding toward zero.
  ///
  /// Inverse of "multiplying a PTS value by this timebase to get seconds".
  /// Saturates at `i64::MAX` if the duration is absurdly large for this
  /// timebase. Returns `0` if `self.num() == 0` (a degenerate timebase).
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn duration_to_pts(&self, d: Duration) -> i64 {
    let num = self.num as u128;
    if num == 0 {
      return 0;
    }
    let den = self.den.get() as u128;
    // pts_units = duration_ns * den / (num * 1e9)
    let ns = d.as_nanos();
    let pts = ns * den / (num * 1_000_000_000);
    if pts > i64::MAX as u128 {
      i64::MAX
    } else {
      pts as i64
    }
  }
}

impl PartialEq for Timebase {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn eq(&self, other: &Self) -> bool {
    // a.num * b.den == b.num * a.den (cross-multiply; u32 * u32 fits in u64)
    (self.num as u64) * (other.den.get() as u64) == (other.num as u64) * (self.den.get() as u64)
  }
}

impl Hash for Timebase {
  fn hash<H: Hasher>(&self, state: &mut H) {
    let d = self.den.get();
    // gcd(num, d) ≥ 1 because d ≥ 1 (NonZeroU32).
    let g = gcd_u32(self.num, d);
    (self.num / g).hash(state);
    (d / g).hash(state);
  }
}

impl Ord for Timebase {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn cmp(&self, other: &Self) -> Ordering {
    let lhs = (self.num as u64) * (other.den.get() as u64);
    let rhs = (other.num as u64) * (self.den.get() as u64);
    lhs.cmp(&rhs)
  }
}

impl PartialOrd for Timebase {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

/// A presentation timestamp, expressed as a PTS value in units of an associated [`Timebase`].
///
/// # Equality and ordering
///
/// Comparison is **value-based** (same instant compares equal even across
/// different timebases): `Timestamp(1000, 1/1000)` equals
/// `Timestamp(90_000, 1/90_000)`. [`Hash`] hashes the reduced-form rational
/// instant `(pts · num, den)`, so equal timestamps hash the same.
///
/// Cross-timebase comparisons use 128-bit cross-multiplication — no division,
/// no rounding error. Same-timebase comparisons take a fast path on `pts`.
#[derive(Debug, Clone, Copy)]
pub struct Timestamp {
  pts: i64,
  timebase: Timebase,
}

impl Timestamp {
  /// Creates a new `Timestamp` with the given PTS and timebase.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn new(pts: i64, timebase: Timebase) -> Self {
    Self { pts, timebase }
  }

  /// Returns the presentation timestamp, in units of [`Self::timebase`].
  ///
  /// To obtain a [`Duration`], use [`Self::duration_since`] against a reference
  /// timestamp, or rescale via [`Self::rescale_to`].
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn pts(&self) -> i64 {
    self.pts
  }

  /// Returns the timebase of the timestamp.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn timebase(&self) -> Timebase {
    self.timebase
  }

  /// Set the value of the presentation timestamp.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn with_pts(mut self, pts: i64) -> Self {
    self.set_pts(pts);
    self
  }

  /// Set the value of the presentation timestamp in place.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn set_pts(&mut self, pts: i64) -> &mut Self {
    self.pts = pts;
    self
  }

  /// Returns a new `Timestamp` representing the same instant in a different timebase.
  ///
  /// Rounds toward zero via [`Timebase::rescale_pts`]; round-tripping through a
  /// coarser timebase can lose precision.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn rescale_to(self, target: Timebase) -> Self {
    Self {
      pts: self.timebase.rescale(self.pts, target),
      timebase: target,
    }
  }

  /// Returns a new [`Timestamp`] representing this instant shifted backward
  /// by `d`, in the same timebase. Saturates at `i64::MIN` if the subtraction
  /// would underflow (pathological for real video).
  ///
  /// Useful for "virtual past" seeding: e.g., initializing a warmup-filter
  /// state to `ts - min_duration` so the first detected cut can fire
  /// immediately.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn saturating_sub_duration(self, d: Duration) -> Self {
    let units = self.timebase.duration_to_pts(d);
    Self::new(self.pts.saturating_sub(units), self.timebase)
  }

  /// `const fn` form of [`Ord::cmp`]. Compares two timestamps by the instant
  /// they represent, rescaling if timebases differ.
  ///
  /// Uses a 128-bit cross-multiply for the mixed-timebase case; no division,
  /// so no rounding error. Same-timebase comparisons take a direct fast path.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn cmp_semantic(&self, other: &Self) -> Ordering {
    if self.timebase.num == other.timebase.num
      && self.timebase.den.get() == other.timebase.den.get()
    {
      return if self.pts < other.pts {
        Ordering::Less
      } else if self.pts > other.pts {
        Ordering::Greater
      } else {
        Ordering::Equal
      };
    }
    // self.pts * self.num / self.den  vs  other.pts * other.num / other.den
    //   ⇔ self.pts * self.num * other.den  vs  other.pts * other.num * self.den
    let lhs = (self.pts as i128) * (self.timebase.num as i128) * (other.timebase.den.get() as i128);
    let rhs =
      (other.pts as i128) * (other.timebase.num as i128) * (self.timebase.den.get() as i128);
    if lhs < rhs {
      Ordering::Less
    } else if lhs > rhs {
      Ordering::Greater
    } else {
      Ordering::Equal
    }
  }

  /// Returns the elapsed [`Duration`] from `earlier` to `self`, or `None` if
  /// `earlier` is after `self`.
  ///
  /// Works across different timebases. Computes the exact rational difference
  /// first using a common denominator, then truncates once when converting to
  /// nanoseconds for the returned [`Duration`].
  /// If the result would exceed `Duration::MAX` (pathological: seconds don't
  /// fit in `u64`), saturates to `Duration::MAX` rather than wrapping.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn duration_since(&self, earlier: &Self) -> Option<Duration> {
    const NS_PER_SEC: i128 = 1_000_000_000;

    // Compute LCM of the two denominators via GCD so we can subtract in a
    // common timebase without per-endpoint truncation.
    let self_den = self.timebase.den.get();
    let earlier_den = earlier.timebase.den.get();

    let mut a = self_den;
    let mut b = earlier_den;
    while b != 0 {
      let r = a % b;
      a = b;
      b = r;
    }
    let gcd = a as i128;

    let self_scale = (earlier_den as i128) / gcd;
    let earlier_scale = (self_den as i128) / gcd;
    let common_den = (self_den as i128) * self_scale; // = lcm(self_den, earlier_den)

    // Exact rational difference in units of 1/common_den seconds.
    let diff_num = (self.pts as i128) * (self.timebase.num as i128) * self_scale
      - (earlier.pts as i128) * (earlier.timebase.num as i128) * earlier_scale;
    if diff_num < 0 {
      return None;
    }

    // Single truncation: convert to whole seconds + nanosecond remainder.
    let secs_i128 = diff_num / common_den;
    if secs_i128 > u64::MAX as i128 {
      return Some(Duration::MAX);
    }
    let rem = diff_num % common_den;
    let nanos = (rem * NS_PER_SEC / common_den) as u32;
    Some(Duration::new(secs_i128 as u64, nanos))
  }
}

impl PartialEq for Timestamp {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn eq(&self, other: &Self) -> bool {
    self.cmp_semantic(other).is_eq()
  }
}
impl Eq for Timestamp {}

impl Hash for Timestamp {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn hash<H: Hasher>(&self, state: &mut H) {
    // Canonical representation: instant as reduced rational (pts * num, den).
    let n: i128 = (self.pts as i128) * (self.timebase.num as i128);
    let d: u128 = self.timebase.den.get() as u128;
    // gcd operates on magnitudes; denominator stays positive. gcd ≥ 1 since d ≥ 1.
    let g = gcd_u128(n.unsigned_abs(), d) as i128;
    let rn = n / g;
    let rd = (d as i128) / g;
    rn.hash(state);
    rd.hash(state);
  }
}

impl Ord for Timestamp {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn cmp(&self, other: &Self) -> Ordering {
    self.cmp_semantic(other)
  }
}
impl PartialOrd for Timestamp {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

/// A half-open time range `[start, end)` in a given [`Timebase`].
///
/// Represents the extent of a detected event — for example, a fade-out →
/// fade-in span. When `start == end`, the range is degenerate (an instant);
/// see [`Self::instant`].
///
/// Both endpoints share the same [`Timebase`]. To compare ranges across
/// different timebases, rescale one of them first (e.g., by calling
/// [`Timestamp::rescale_to`] on each endpoint).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimeRange {
  start: i64,
  end: i64,
  timebase: Timebase,
}

impl TimeRange {
  /// Creates a new `TimeRange` with the given start/end PTS and shared timebase.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn new(start: i64, end: i64, timebase: Timebase) -> Self {
    Self {
      start,
      end,
      timebase,
    }
  }

  /// Creates a degenerate (instant) range where `start == end == ts.pts()`.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn instant(ts: Timestamp) -> Self {
    Self {
      start: ts.pts(),
      end: ts.pts(),
      timebase: ts.timebase(),
    }
  }

  /// Returns the start PTS in the range's timebase units.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn start_pts(&self) -> i64 {
    self.start
  }

  /// Returns the end PTS in the range's timebase units.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn end_pts(&self) -> i64 {
    self.end
  }

  /// Returns the shared timebase.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn timebase(&self) -> Timebase {
    self.timebase
  }

  /// Returns the start as a [`Timestamp`].
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn start(&self) -> Timestamp {
    Timestamp::new(self.start, self.timebase)
  }

  /// Returns the end as a [`Timestamp`].
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn end(&self) -> Timestamp {
    Timestamp::new(self.end, self.timebase)
  }

  /// Sets the start PTS.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn with_start(mut self, val: i64) -> Self {
    self.start = val;
    self
  }

  /// Sets the start PTS in place.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn set_start(&mut self, val: i64) -> &mut Self {
    self.start = val;
    self
  }

  /// Sets the end PTS.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn with_end(mut self, val: i64) -> Self {
    self.end = val;
    self
  }

  /// Sets the end PTS in place.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn set_end(&mut self, val: i64) -> &mut Self {
    self.end = val;
    self
  }

  /// Returns `true` if `start == end` (a degenerate instant range).
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn is_instant(&self) -> bool {
    self.start == self.end
  }

  /// Returns the elapsed [`Duration`] from `start` to `end`, or `None` if
  /// `end` is before `start`.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn duration(&self) -> Option<Duration> {
    self.end().duration_since(&self.start())
  }

  /// Linearly interpolates between `start` and `end`: `t = 0.0` returns
  /// `start`, `t = 1.0` returns `end`, `t = 0.5` the midpoint. `t` is
  /// clamped to `[0.0, 1.0]`. Rounds toward zero.
  ///
  /// Use this to map an old-style bias value `b ∈ [-1, 1]` onto the range:
  /// `range.interpolate((b + 1.0) * 0.5)`.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn interpolate(&self, t: f64) -> Timestamp {
    let t = t.clamp(0.0, 1.0);
    let delta = self.end.saturating_sub(self.start);
    let offset = (delta as f64 * t) as i64;
    Timestamp::new(self.start.saturating_add(offset), self.timebase)
  }
}

#[cfg_attr(not(tarpaulin), inline(always))]
const fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
  while b != 0 {
    let t = b;
    b = a % b;
    a = t;
  }
  a
}

#[cfg_attr(not(tarpaulin), inline(always))]
const fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
  while b != 0 {
    let t = b;
    b = a % b;
    a = t;
  }
  a
}

#[cfg(test)]
mod tests {
  use super::*;

  const fn nz(n: u32) -> NonZeroU32 {
    match NonZeroU32::new(n) {
      Some(v) => v,
      None => panic!("zero"),
    }
  }

  fn hash_of<T: Hash>(v: &T) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
  }

  #[test]
  fn rescale_identity() {
    let tb = Timebase::new(1, nz(1000));
    assert_eq!(Timebase::rescale_pts(42, tb, tb), 42);
    assert_eq!(tb.rescale(42, tb), 42);
  }

  #[test]
  fn rescale_between_timebases() {
    let ms = Timebase::new(1, nz(1000));
    let mpeg = Timebase::new(1, nz(90_000));
    assert_eq!(Timebase::rescale_pts(1000, ms, mpeg), 90_000);
    assert_eq!(ms.rescale(1000, mpeg), 90_000);
    assert_eq!(mpeg.rescale(90_000, ms), 1000);
  }

  #[test]
  fn rescale_rounds_toward_zero() {
    let from = Timebase::new(1, nz(1000));
    let to = Timebase::new(1, nz(3));
    assert_eq!(from.rescale(1, to), 0);
    assert_eq!(from.rescale(-1, to), 0);
  }

  #[test]
  fn rescale_saturates_on_i64_overflow() {
    // Rescale from a coarse timebase (u32::MAX seconds per tick) to a fine
    // one (1/u32::MAX seconds per tick): even a modest pts blows past
    // i64::MAX in the 128-bit intermediate. `rescale_pts` should saturate
    // to i64::MAX / i64::MIN rather than wrap via `as i64`.
    let from = Timebase::new(u32::MAX, nz(1));
    let to = Timebase::new(1, nz(u32::MAX));
    assert_eq!(from.rescale(1_000_000, to), i64::MAX);
    assert_eq!(from.rescale(-1_000_000, to), i64::MIN);
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
    // Use a timebase of `u32::MAX / 1` (each tick ≈ 2^32 seconds). Then
    // i64::MAX ticks ≈ 2^95 seconds — far more than u64::MAX. Should
    // saturate to Duration::MAX rather than wrap when casting seconds to u64.
    let tb = Timebase::new(u32::MAX, nz(1));
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
    let tb = Timebase::new(1, nz(1000));
    let r = TimeRange::new(100, 500, tb);
    assert_eq!(r.start_pts(), 100);
    assert_eq!(r.end_pts(), 500);
    assert_eq!(r.timebase(), tb);
    assert_eq!(r.start(), Timestamp::new(100, tb));
    assert_eq!(r.end(), Timestamp::new(500, tb));
    assert!(!r.is_instant());
    assert_eq!(r.duration(), Some(Duration::from_millis(400)));
    // Interpolate: t=0 → start, t=1 → end, t=0.5 → midpoint.
    assert_eq!(r.interpolate(0.0).pts(), 100);
    assert_eq!(r.interpolate(1.0).pts(), 500);
    assert_eq!(r.interpolate(0.5).pts(), 300);
    // Out-of-range t is clamped.
    assert_eq!(r.interpolate(-1.0).pts(), 100);
    assert_eq!(r.interpolate(2.0).pts(), 500);
  }

  #[test]
  fn time_range_instant() {
    let tb = Timebase::new(1, nz(1000));
    let ts = Timestamp::new(123, tb);
    let r = TimeRange::instant(ts);
    assert!(r.is_instant());
    assert_eq!(r.start_pts(), 123);
    assert_eq!(r.end_pts(), 123);
    assert_eq!(r.duration(), Some(Duration::ZERO));
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
  fn duration_to_pts_happy_path_and_edge_cases() {
    // Integer conversion: 1.5 s @ 1/1000 → 1500 units.
    let ms = Timebase::new(1, nz(1000));
    assert_eq!(ms.duration_to_pts(Duration::from_millis(1500)), 1500);
    assert_eq!(ms.duration_to_pts(Duration::ZERO), 0);

    // Non-ms timebase: 2 s @ 1/90_000 → 180_000 units.
    let mpegts = Timebase::new(1, nz(90_000));
    assert_eq!(mpegts.duration_to_pts(Duration::from_secs(2)), 180_000,);

    // Degenerate: zero numerator → returns 0.
    let degenerate = Timebase::new(0, nz(1));
    assert_eq!(degenerate.duration_to_pts(Duration::from_secs(1)), 0,);

    // Saturation at i64::MAX when the math would overflow.
    // A frame rate of 1 fps (num=1, den=1 s) with an enormous duration:
    // pts = ns * 1 / (1 * 1e9). Use a u64::MAX-ish nanos value via the
    // max Duration; Rust's Duration max is ~(2^64 - 1) seconds.
    let fps1 = Timebase::new(1, nz(1));
    let huge = Duration::new(u64::MAX, 0);
    assert_eq!(fps1.duration_to_pts(huge), i64::MAX);
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

    // Reversed range: end before start means duration() is None.
    let reversed = TimeRange::new(500, 100, tb);
    assert!(reversed.duration().is_none());
  }
}
