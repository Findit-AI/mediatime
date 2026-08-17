#![doc = include_str!("../README.md")]
#![cfg_attr(not(test), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, allow(unused_attributes))]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

// `quickcheck` itself is std-only; the `quickcheck` feature implicitly pulls
// std in, but the `no_std` attribute up top means `::std::*` paths still need
// the crate brought into scope explicitly so `quickcheck-richderive`'s
// generated `::std::boxed::Box<…>` shrink type resolves.
#[cfg(feature = "quickcheck")]
#[allow(unused_extern_crates)]
extern crate std;

use core::{
  cmp::Ordering,
  fmt,
  hash::{Hash, Hasher},
  num::NonZeroI32,
  time::Duration,
};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

mod parse;

pub use parse::{ParseTimeRangeError, ParseTimebaseError, ParseTimestampError};

/// Nanoseconds in a second — the factor that turns a [`Duration`] into ticks
/// of a [`Timebase`] and back.
const NANOS_PER_SEC: u128 = 1_000_000_000;

/// `NonZeroI32` from a denominator known at compile time, for the const
/// contexts `NonZeroI32::new(..).expect(..)` cannot be written in without
/// naming a message no caller will ever read.
///
/// # Panics
///
/// Panics if `n == 0`, which every call site rules out by passing a literal.
const fn nz(n: i32) -> NonZeroI32 {
  match NonZeroI32::new(n) {
    Some(v) => v,
    None => unreachable!(),
  }
}

/// `NonZeroI32` for 1: the default denominator, and the clamp target when a
/// malformed denominator arrives on the wire.
///
/// Spelled out rather than reached for as `NonZeroI32::MIN`, which is
/// `i32::MIN` — a value [`Timebase::new`] rejects.
pub(crate) const DEN_ONE: NonZeroI32 = nz(1);

/// A media timebase represented as a rational number: a non-negative numerator
/// over a strictly positive denominator.
///
/// Typical values: `1/1000` for millisecond PTS, `1/90000` for MPEG-TS,
/// `1/48000` for audio samples, `30000/1001` for NTSC video (when used as a
/// frame rate).
///
/// # Why both halves are signed
///
/// FFmpeg's rational is signed — `AVRational { int num; int den; }` — and it is
/// the type this crate exists to interoperate with: `av_rescale_q` takes two of
/// them, `AVFrame::time_base` is one, and `AVFrame::pts` is an `int64_t` whose
/// `AV_NOPTS_VALUE` sentinel is `i64::MIN`, so signedness is load-bearing
/// throughout that API. An unsigned numerator or denominator above `i32::MAX`
/// is representable but **cannot round-trip into an `AVRational`** — usable in
/// Rust, unusable at the boundary. Matching the width and the sign removes that
/// failure mode by construction.
///
/// Storage points the same way: `sqlx` has no `Type<Postgres>`/`Encode<Postgres>`
/// for `u32`, whereas `i32` is a native `INTEGER` on PostgreSQL, MySQL and
/// SQLite alike, so a storage face reads these fields directly instead of
/// widening to `i64` and narrowing back through an error path.
///
/// Two other decoder SDKs were surveyed and impose no counter-pressure:
/// Blackmagic RAW (`GetFrameRate(float*)`) and RED R3D
/// (`float VideoAudioFramerate()`) are frame-indexed with a floating-point
/// rate and never hand out a rational at all.
///
/// # Invariants
///
/// `num >= 0` and `den > 0`. `NonZeroI32` carries only the non-zero half, so
/// the rest is enforced by [`Timebase::new`] (and by every setter, which routes
/// through it). A **zero numerator stays legal**: it is a degenerate timebase,
/// valid to construct and to compare, but not a valid rescale target — see
/// [`Timebase::checked_rescale`].
///
/// `AVRational` itself permits a negative denominator and normalizes the sign
/// into the numerator via `av_reduce`; that is a convention rather than a type
/// guarantee, and `AVRational` is laxer than this crate needs because it also
/// serves aspect ratios. Here it is a type-level guarantee instead.
///
/// # Equality and ordering
///
/// Comparison is **value-based**: `1/2` equals `2/4`, and `1/3 < 2/3 < 1/1`.
/// [`Hash`] hashes the reduced (lowest-terms) form, so equal rationals hash
/// the same. Cross-multiplication uses `i64` intermediates — exact for any
/// `i32` numerator / denominator.
///
/// # The well-known roster
///
/// The constants on this type — [`MILLIS`](Self::MILLIS),
/// [`MPEG_90K`](Self::MPEG_90K), [`NTSC_VIDEO`](Self::NTSC_VIDEO) and the
/// rest — are the timebases containers and codecs actually declare, each with
/// a name [`Self::from_name`] reads and [`Self::well_known_name`] writes back.
///
/// Every one of them is a **timebase**: seconds per tick. The frame-rate
/// entries are therefore the *reciprocals* of the rate they are named for —
/// [`FILM_24`](Self::FILM_24) is `1/24`, not `24/1` — because a PTS timebase
/// and a frame rate are reciprocal readings of the same rational, as
/// [`Self::frames_to_duration`] describes. [`Self::checked_recip`] converts
/// between the two readings.
#[derive(Debug, Clone, Copy, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "quickcheck",
  derive(::quickcheck_richderive::Arbitrary),
  quickcheck(arbitrary = "crate::quickcheck_impls::timebase")
)]
pub struct Timebase {
  #[cfg_attr(
    feature = "serde",
    serde(rename = "numerator", deserialize_with = "de_num")
  )]
  num: i32,
  #[cfg_attr(
    feature = "serde",
    serde(rename = "denominator", deserialize_with = "de_den")
  )]
  den: NonZeroI32,
}

impl Default for Timebase {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn default() -> Self {
    Self::new(1, DEN_ONE)
  }
}

impl Timebase {
  /// One tick per second — the coarsest of the roster, and the timebase a
  /// value already counted in whole seconds carries.
  pub const SECONDS: Self = Self::new(1, nz(1));

  /// Millisecond ticks — Matroska's default `TimecodeScale` (1 000 000 ns),
  /// WebVTT and SRT cue times, and the unit most application-level media APIs
  /// report positions in.
  pub const MILLIS: Self = Self::new(1, nz(1_000));

  /// Microsecond ticks — FFmpeg's `AV_TIME_BASE`, which is the unit
  /// `AVFormatContext::duration` and `av_seek_frame`'s default are expressed
  /// in (`AV_TIME_BASE_Q` is exactly this rational).
  pub const MICROS: Self = Self::new(1, nz(1_000_000));

  /// Nanosecond ticks — the resolution [`Duration`] itself carries, so this
  /// is the timebase a [`Duration`] is already counted in, and the one a
  /// conversion to or from one rescales through.
  pub const NANOS: Self = Self::new(1, nz(1_000_000_000));

  /// The 90 kHz clock MPEG counts PTS and DTS in — MPEG-TS, MPEG-PS, and
  /// RTP's video clock rate all use it.
  pub const MPEG_90K: Self = Self::new(1, nz(90_000));

  /// One tick per audio sample at 48 kHz — DVD and broadcast audio,
  /// professional interchange, and Opus, whose clock rate is always 48 kHz.
  pub const HZ_48K: Self = Self::new(1, nz(48_000));

  /// One tick per audio sample at 44.1 kHz — CD-DA's rate, and the one most
  /// MP3 and AAC music files carry.
  pub const HZ_44_1K: Self = Self::new(1, nz(44_100));

  /// One tick per frame at 24000/1001 fps (`23.976`) — film pulled down for
  /// NTSC, which is what most film-sourced MP4 and MOV files declare.
  ///
  /// The reciprocal of the frame rate, per the [roster's
  /// note](Self#the-well-known-roster).
  pub const NTSC_FILM: Self = Self::new(1_001, nz(24_000));

  /// One tick per frame at 30000/1001 fps (`29.97`) — NTSC video, and the
  /// rate broadcast-sourced material in North America and Japan carries.
  ///
  /// The reciprocal of the frame rate, per the [roster's
  /// note](Self#the-well-known-roster).
  pub const NTSC_VIDEO: Self = Self::new(1_001, nz(30_000));

  /// One tick per frame at exactly 24 fps — cinema's rate, and what a DCP
  /// counts in.
  ///
  /// The reciprocal of the frame rate, per the [roster's
  /// note](Self#the-well-known-roster).
  pub const FILM_24: Self = Self::new(1, nz(24));

  /// One tick per frame at 25 fps — PAL and SECAM broadcast, and EBU
  /// timecode.
  ///
  /// The reciprocal of the frame rate, per the [roster's
  /// note](Self#the-well-known-roster).
  pub const PAL_25: Self = Self::new(1, nz(25));

  /// Creates a new `Timebase` with the given numerator and denominator.
  ///
  /// # Panics
  ///
  /// - Panics if `num < 0` (a negative timebase is meaningless).
  /// - Panics if `den <= 0` (`NonZeroI32` rules out zero; this rules out the
  ///   negative denominators `AVRational` would tolerate).
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn new(num: i32, den: NonZeroI32) -> Self {
    assert!(num >= 0, "timebase numerator must not be negative");
    assert!(den.get() > 0, "timebase denominator must be positive");

    Self { num, den }
  }

  /// Fallible variant of [`Self::new`]: returns `None` instead of panicking
  /// when `num < 0` or `den < 0`. Accepts `num == 0` (degenerate timebase).
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn try_new(num: i32, den: NonZeroI32) -> Option<Self> {
    if num >= 0 && den.get() > 0 {
      Some(Self { num, den })
    } else {
      None
    }
  }

  /// Looks up a [well-known timebase](Self#the-well-known-roster) by the exact
  /// name of its constant — `"MILLIS"`, `"MPEG_90K"`, `"NTSC_VIDEO"`.
  ///
  /// The match is exact and case-sensitive: the names are the constants'
  /// own `SCREAMING_SNAKE_CASE` spellings, so a name that round-trips through
  /// [`Self::well_known_name`] is the one a reader can grep for in this file.
  /// `None` for anything else — including a `num/den` rendering, which
  /// [`FromStr`](core::str::FromStr) accepts on its other arm.
  pub fn from_name(name: &str) -> Option<Self> {
    WELL_KNOWN
      .iter()
      .find_map(|(known, timebase)| (*known == name).then_some(*timebase))
  }

  /// The name of the [well-known timebase](Self#the-well-known-roster) this
  /// one *equals*, if any — the inverse of [`Self::from_name`].
  ///
  /// Matched **by value**, as [`PartialEq`] matches: `2/2000` is
  /// [`MILLIS`](Self::MILLIS) and answers to that name, even though
  /// [`Display`](fmt::Display) will still print the `2/2000` the stream
  /// declared. No two roster entries are equal, so the answer is unambiguous.
  ///
  /// Written for an output face that wants to *say* which timebase a stream
  /// carries rather than hand a reader two integers to divide.
  pub fn well_known_name(&self) -> Option<&'static str> {
    WELL_KNOWN
      .iter()
      .find_map(|(name, timebase)| (timebase == self).then_some(*name))
  }

  /// Returns the numerator.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn num(&self) -> i32 {
    self.num
  }

  /// Returns the denominator.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn den(&self) -> NonZeroI32 {
    self.den
  }

  /// Set the value of the numerator.
  ///
  /// # Panics
  ///
  /// Panics if `num < 0`, as [`Self::new`] does.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn with_num(mut self, num: i32) -> Self {
    self.set_num(num);
    self
  }

  /// Set the value of the denominator.
  ///
  /// # Panics
  ///
  /// Panics if `den < 0`, as [`Self::new`] does.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn with_den(mut self, den: NonZeroI32) -> Self {
    self.set_den(den);
    self
  }

  /// Set the value of the numerator in place.
  ///
  /// # Panics
  ///
  /// Panics if `num < 0`, as [`Self::new`] does.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn set_num(&mut self, num: i32) -> &mut Self {
    // Routed through the constructor so the sign invariants have exactly one
    // enforcement site; the arithmetic below relies on them holding for every
    // reachable `Timebase`, not just constructed-and-never-mutated ones.
    *self = Self::new(num, self.den);
    self
  }

  /// Set the value of the denominator in place.
  ///
  /// # Panics
  ///
  /// Panics if `den < 0`, as [`Self::new`] does.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn set_den(&mut self, den: NonZeroI32) -> &mut Self {
    *self = Self::new(self.num, den);
    self
  }

  /// Reduces the rational to lowest terms: `2/4` becomes `1/2`, `0/3` becomes
  /// `0/1`.
  ///
  /// The value is unchanged — the reduced form compares equal to what it came
  /// from and hashes with it — so this is a *canonicalization*, useful where a
  /// declared form has to be stored or rendered once per distinct value rather
  /// than once per way of writing it. [`Display`](fmt::Display) deliberately
  /// does **not** reduce.
  ///
  /// No sign handling: `num >= 0` and `den > 0` are constructor invariants, so
  /// the gcd of the magnitudes is the gcd, and it is at least 1 because
  /// `den >= 1`.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn reduce(self) -> Self {
    let g = gcd_u32(self.num.unsigned_abs(), self.den.get().unsigned_abs()) as i32;
    Self {
      num: self.num / g,
      den: nz(self.den.get() / g),
    }
  }

  /// Whether the rational is already in lowest terms — `true` for `1/2` and
  /// `0/1`, `false` for `2/4` and `0/3`.
  ///
  /// Exactly `*self == self.reduce()` in the *structural* sense that `==`
  /// itself cannot express, `==` being value-based here.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn is_reduced(&self) -> bool {
    gcd_u32(self.num.unsigned_abs(), self.den.get().unsigned_abs()) == 1
  }

  /// The reciprocal — `1/24` becomes `24/1` — or `None` when the numerator is
  /// zero and no reciprocal exists.
  ///
  /// This is the conversion between the two readings of a `Timebase`: a PTS
  /// timebase (seconds per tick) and a frame rate (frames per second) are
  /// reciprocals, which is why the [roster](Self#the-well-known-roster) spells
  /// [`FILM_24`](Self::FILM_24) as `1/24` and
  /// [`Self::frames_to_duration`] wants `24/1`.
  ///
  /// A zero numerator is the only failure: the swap is otherwise total,
  /// because the constructor's `den > 0` becomes the new numerator's
  /// `num >= 0` and a `num > 0` becomes a legal denominator.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn checked_recip(self) -> Option<Self> {
    match NonZeroI32::new(self.num) {
      Some(den) => Some(Self {
        num: self.den.get(),
        den,
      }),
      None => None,
    }
  }

  /// Rescales `pts` from this timebase to `to`, or `None` if the answer is not
  /// an `i64`.
  ///
  /// `self` is the source timebase, so this is FFmpeg's
  /// `av_rescale_q(pts, self, to)` — including its rounding, which is
  /// **to nearest, halfway cases away from zero** (`AV_ROUND_NEAR_INF`, the
  /// posture `av_rescale` and `av_rescale_q` take by default). Rescaling
  /// `1/1000` ticks into `1/3` ticks sends `500` to `2` rather than to `1`.
  ///
  /// The product is formed in `i128`, which cannot overflow: the operands are
  /// bounded by `2^63`, `2^31` and `2^31`, so the intermediate stays under
  /// `2^125`. Two things are then reported as `None` rather than answered
  /// wrongly:
  ///
  /// - a quotient outside `i64`'s range (pathological for real video);
  /// - a `to` whose numerator is zero — a degenerate timebase names one single
  ///   instant, so no tick count in it can represent a non-zero one.
  ///
  /// [`Self::saturating_rescale`] is the same arithmetic with the other
  /// posture toward the first of those.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn checked_rescale(&self, pts: i64, to: Self) -> Option<i64> {
    if to.num == 0 {
      return None;
    }
    let q = rescaled(pts, *self, to);
    if q > i64::MAX as i128 || q < i64::MIN as i128 {
      None
    } else {
      Some(q as i64)
    }
  }

  /// Rescales `pts` from this timebase to `to`, clamping to `i64::MIN` or
  /// `i64::MAX` instead of overflowing.
  ///
  /// The saturating rung of [`Self::checked_rescale`]: same arithmetic, same
  /// rounding (to nearest, halfway cases away from zero), and the only
  /// difference is that a quotient too large for an `i64` comes back as the
  /// nearest `i64` rather than as `None`.
  ///
  /// # Panics
  ///
  /// Panics if `to.num() == 0`, the divide-by-zero a degenerate target
  /// timebase would be — as [`i64::saturating_div`] panics on a zero divisor,
  /// and for the same reason: saturation is a posture toward *overflow*, and
  /// there is no quotient here to clamp. Use [`Self::checked_rescale`] where
  /// the target may be degenerate.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn saturating_rescale(&self, pts: i64, to: Self) -> i64 {
    assert!(to.num != 0, "target timebase numerator must be non-zero");
    let q = rescaled(pts, *self, to);
    if q > i64::MAX as i128 {
      i64::MAX
    } else if q < i64::MIN as i128 {
      i64::MIN
    } else {
      q as i64
    }
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
    //
    // `as u128` widens rather than sign-extends only because the constructor
    // guarantees `num >= 0` and `den > 0`; a negative operand here would
    // become an enormous positive one.
    let num = self.num as u128;
    let den = self.den.get() as u128;
    assert!(num != 0, "frame rate numerator must be non-zero");
    let total_ns = (frames as u128) * den * NANOS_PER_SEC / num;
    let secs = (total_ns / NANOS_PER_SEC) as u64;
    let nanos = (total_ns % NANOS_PER_SEC) as u32;
    Duration::new(secs, nanos)
  }

  /// Converts a [`Duration`] into the number of ticks of this timebase that
  /// span it, or `None` if that count is not an `i64`.
  ///
  /// The inverse of [`Self::checked_pts_to_duration`], and the same conversion
  /// [`Self::checked_rescale`] performs out of [`NANOS`](Self::NANOS) — with
  /// the same rounding, to nearest with halfway cases away from zero. Since a
  /// [`Duration`] is never negative, "away from zero" is "up" here.
  ///
  /// Two things come back as `None` rather than as a wrong answer:
  ///
  /// - a count too large for an `i64` (the duration is absurd for this
  ///   timebase);
  /// - a `self.num() == 0` degenerate timebase, whose every tick lands on the
  ///   same instant, so no count of them spans a non-zero duration.
  ///
  /// The second is where this rung earns its keep: it is the only spelling of
  /// the conversion that answers at all on a degenerate timebase, its twin
  /// [`Self::saturating_duration_to_pts`] panicking there.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn checked_duration_to_pts(&self, d: Duration) -> Option<i64> {
    if self.num == 0 {
      return None;
    }
    let ticks = self.duration_ticks(d);
    if ticks > i64::MAX as u128 {
      None
    } else {
      Some(ticks as i64)
    }
  }

  /// Converts a [`Duration`] into the number of ticks of this timebase that
  /// span it, clamping at `i64::MAX` instead of overflowing.
  ///
  /// The saturating rung of [`Self::checked_duration_to_pts`]: same
  /// arithmetic, same rounding, and a count too large for an `i64` comes back
  /// as `i64::MAX`.
  ///
  /// # Panics
  ///
  /// Panics if `self.num() == 0`, the divide-by-zero a degenerate timebase
  /// would be — the same posture [`Self::saturating_rescale`] takes toward the
  /// same degeneracy, and for the reason [`i64::saturating_div`] panics on a
  /// zero divisor: saturation answers *overflow*, and a timebase whose every
  /// tick lands on one instant leaves no count to clamp. Use
  /// [`Self::checked_duration_to_pts`] where the timebase may be degenerate.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn saturating_duration_to_pts(&self, d: Duration) -> i64 {
    assert!(self.num != 0, "target timebase numerator must be non-zero");
    let ticks = self.duration_ticks(d);
    if ticks > i64::MAX as u128 {
      i64::MAX
    } else {
      ticks as i64
    }
  }

  /// Converts a tick count in this timebase into the [`Duration`] it spans, or
  /// `None` if no [`Duration`] represents it.
  ///
  /// The inverse of [`Self::checked_duration_to_pts`], rounded to the nearest
  /// nanosecond with halfway cases away from zero. Two things come back as
  /// `None`:
  ///
  /// - a negative `pts`, which pre-roll and edit lists produce and which
  ///   [`Duration`] cannot represent (it is unsigned) — see
  ///   [`Timestamp::duration`] for the same refusal on a whole timestamp;
  /// - a span whose seconds exceed `u64::MAX`, past [`Duration::MAX`].
  ///
  /// A degenerate `self.num() == 0` timebase is **not** a failure in this
  /// direction: every tick of it lands on the same instant, and
  /// [`Duration::ZERO`] is that instant.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn checked_pts_to_duration(&self, pts: i64) -> Option<Duration> {
    if pts < 0 {
      return None;
    }
    let nanos = self.tick_nanos(pts);
    let secs = nanos / NANOS_PER_SEC;
    if secs > u64::MAX as u128 {
      return None;
    }
    Some(Duration::new(secs as u64, (nanos % NANOS_PER_SEC) as u32))
  }

  /// Converts a tick count in this timebase into the [`Duration`] it spans,
  /// clamping at both ends of what a [`Duration`] can hold.
  ///
  /// The saturating rung of [`Self::checked_pts_to_duration`]: same
  /// arithmetic, same rounding. A negative `pts` clamps to [`Duration::ZERO`]
  /// and a span past [`Duration::MAX`] clamps to it — the two bounds of the
  /// type, which is what saturation means for a type that has no negative
  /// half.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn saturating_pts_to_duration(&self, pts: i64) -> Duration {
    if pts < 0 {
      return Duration::ZERO;
    }
    let nanos = self.tick_nanos(pts);
    let secs = nanos / NANOS_PER_SEC;
    if secs > u64::MAX as u128 {
      return Duration::MAX;
    }
    Duration::new(secs as u64, (nanos % NANOS_PER_SEC) as u32)
  }

  /// `d` in ticks of this timebase, rounded to nearest with halfway cases up,
  /// before either rung decides what to do with a count too large for an
  /// `i64`. `self.num` must be non-zero.
  ///
  /// Every operand is non-negative — `num >= 0` and `den > 0` by construction,
  /// and a [`Duration`] is unsigned — so `as u128` widens rather than
  /// sign-extends, and "away from zero" is "up".
  ///
  /// The widest intermediate is `Duration::MAX` in nanoseconds (just under
  /// `2^64 * 10^9`, so under `2^94`) times an `i32::MAX` denominator: under
  /// `2^125`, against `u128`'s `2^128`.
  #[cfg_attr(not(tarpaulin), inline(always))]
  const fn duration_ticks(&self, d: Duration) -> u128 {
    // ticks = duration_ns * den / (num * 1e9)
    let numerator = d.as_nanos() * (self.den.get() as u128);
    let denominator = (self.num as u128) * NANOS_PER_SEC;
    div_round_half_up(numerator, denominator)
  }

  /// A `pts` in nanoseconds under this timebase, rounded to nearest with
  /// halfway cases up, before either rung decides what to do with a span too
  /// long for a [`Duration`]. `pts` must be non-negative — both callers refuse
  /// a negative one first, which is what makes `as u128` a widening here.
  ///
  /// The widest intermediate is `i64::MAX` ticks times an `i32::MAX` numerator
  /// times `10^9`: under `2^124`.
  #[cfg_attr(not(tarpaulin), inline(always))]
  const fn tick_nanos(&self, pts: i64) -> u128 {
    // nanos = pts * num * 1e9 / den
    let numerator = (pts as u128) * (self.num as u128) * NANOS_PER_SEC;
    div_round_half_up(numerator, self.den.get() as u128)
  }
}

impl PartialEq for Timebase {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn eq(&self, other: &Self) -> bool {
    // a.num * b.den == b.num * a.den (cross-multiply; i32 * i32 fits in i64)
    (self.num as i64) * (other.den.get() as i64) == (other.num as i64) * (self.den.get() as i64)
  }
}

impl Hash for Timebase {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn hash<H: Hasher>(&self, state: &mut H) {
    // Equal rationals must hash alike, and `==` here is value-based, so the
    // canonical form is what goes into the hasher. `reduce` is that
    // canonicalization, and reusing it keeps one gcd site rather than two that
    // could drift apart.
    let reduced = self.reduce();
    reduced.num.hash(state);
    reduced.den.get().hash(state);
  }
}

impl Ord for Timebase {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn cmp(&self, other: &Self) -> Ordering {
    let lhs = (self.num as i64) * (other.den.get() as i64);
    let rhs = (other.num as i64) * (self.den.get() as i64);
    lhs.cmp(&rhs)
  }
}

impl PartialOrd for Timebase {
  #[cfg_attr(not(tarpaulin), inline(always))]
  fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
    Some(self.cmp(other))
  }
}

/// Writes the rational as `num/den` — `1/1000`, `1/90000`, `30000/1001`.
///
/// The stored form is printed, **not** the reduced one: `2/4` prints as `2/4`
/// even though it equals `1/2` and hashes with it. In a log the interesting
/// fact is which timebase a stream declared, and reducing would erase the
/// difference between a container that said `30000/1001` and one that said
/// `60000/2002`.
///
/// Unlike [`Timestamp`]'s and [`TimeRange`]'s, this rendering is exact — a
/// numerator and a denominator are the whole value — so `{:#}` renders
/// identically; there is nothing to expand into. [`FromStr`](core::str::FromStr)
/// inverts it.
///
/// Width and alignment flags (`{:>12}`) are ignored: honouring them means
/// measuring the finished string, and this crate has no `alloc` to build one
/// in.
impl fmt::Display for Timebase {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}/{}", self.num, self.den.get())
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
#[derive(Debug, Default, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "quickcheck",
  derive(::quickcheck_richderive::Arbitrary),
  quickcheck(arbitrary = "crate::quickcheck_impls::timestamp")
)]
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
  /// Converts through [`Timebase::saturating_rescale`], so the new PTS is the
  /// nearest tick of `target` (halfway cases away from zero); round-tripping
  /// through a coarser timebase can still lose precision.
  ///
  /// # Panics
  ///
  /// Panics if `target.num() == 0`, as [`Timebase::saturating_rescale`] does:
  /// a degenerate timebase names one instant, and cannot receive another.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn rescale_to(self, target: Timebase) -> Self {
    Self {
      pts: self.timebase.saturating_rescale(self.pts, target),
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
  ///
  /// # Panics
  ///
  /// Panics if `self.timebase().num() == 0`, as
  /// [`Timebase::saturating_duration_to_pts`] does: a degenerate timebase
  /// spans no time per tick, so no tick count stands for `d`.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn saturating_sub_duration(self, d: Duration) -> Self {
    let units = self.timebase.saturating_duration_to_pts(d);
    Self::new(self.pts.saturating_sub(units), self.timebase)
  }

  /// Returns a new [`Timestamp`] representing this instant shifted forward
  /// by `d`, in the same timebase. Saturates at `i64::MAX` if the addition
  /// would overflow (pathological for real video).
  ///
  /// The forward twin of [`Self::saturating_sub_duration`]: use it to close a
  /// window opened at `self`, e.g. `ts + max_gap` for the deadline a
  /// detector will stop waiting at.
  ///
  /// Saturating in both steps, and only the second is visible in the result:
  /// [`Timebase::saturating_duration_to_pts`] itself saturates when `d` is
  /// enormous for this timebase, so a saturated answer can mean either "the
  /// duration did not fit" or "the sum did not". Both say the same thing about
  /// the instant — it is past the end of what an `i64` PTS can name.
  ///
  /// # Panics
  ///
  /// Panics if `self.timebase().num() == 0`, as
  /// [`Timebase::saturating_duration_to_pts`] does: a degenerate timebase
  /// spans no time per tick, so no tick count stands for `d`.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn saturating_add_duration(self, d: Duration) -> Self {
    let units = self.timebase.saturating_duration_to_pts(d);
    Self::new(self.pts.saturating_add(units), self.timebase)
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

  /// Returns the [`Duration`] from PTS zero (in this timebase) to `self`, or
  /// `None` if `self.pts() < 0` (pre-roll / edit-list cases can produce
  /// negative PTS, which has no [`Duration`] representation).
  ///
  /// Equivalent to `self.duration_since(&Timestamp::new(0, self.timebase()))`.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn duration(&self) -> Option<Duration> {
    self.duration_since(&Self::new(0, self.timebase))
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
    //
    // Euclid on signed operands needs both to be positive: Rust's `%` takes
    // the sign of the dividend, so a negative denominator would yield a
    // negative gcd and silently invert the scale factors below. The
    // constructor guarantees `den > 0`.
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
    // Exact widening: the constructor guarantees `den > 0`.
    let d: u128 = self.timebase.den.get().unsigned_abs() as u128;
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

/// Writes the instant on the clock as `H:MM:SS.mmm` — `0:00:00.137` — so a log
/// line reads as a time instead of as a division to carry out. `video-rs`
/// prints the unreduced rational (`12345/90000 secs`) in the same position;
/// the readable form is the default here because readable log messages are
/// what [the request this impl answers][issue] asked for, and the rational is
/// still one `#` away.
///
/// Hours are unpadded and unbounded — `123:45:06.789` is a normal rendering,
/// not an overflow. Minutes and seconds are two digits, milliseconds three. A
/// negative PTS (pre-roll, or an edit list) signs the whole rendering:
/// `-0:00:01.500`.
///
/// The instant is **truncated toward zero** at millisecond resolution, and
/// deliberately not rounded the way the rescale ladder rounds (see
/// [`Timebase::checked_rescale`]): a clock is read as elapsed time, and must
/// not name an instant the stream has not reached yet. So this form is lossy
/// twice over: below a millisecond nothing survives, and the timebase the PTS
/// was counted in is not shown at all. One consequence is worth stating
/// outright — a PTS smaller
/// in magnitude than one millisecond renders `0:00:00.000` *without* a sign,
/// because the value being printed is zero and a signed zero would claim a
/// precision this form does not have.
///
/// `{:#}` is the exact form: the stored PTS beside its timebase, as
/// `12345 @ 1/90000`. So is the derived [`Debug`]. Being the exact one, `{:#}`
/// is also the form [`FromStr`](core::str::FromStr) reads back; the clock is
/// lossy and has no inverse.
///
/// Width and alignment flags (`{:>12}`) are ignored, so this will not line a
/// log up into columns: honouring them means measuring the finished string,
/// and this crate has no `alloc` to build one in.
///
/// [issue]: https://github.com/findit-studio/mediatime/issues/13
impl fmt::Display for Timestamp {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if f.alternate() {
      write!(f, "{} @ {}", self.pts, self.timebase)
    } else {
      write_clock(f, self.pts, self.timebase)
    }
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
  feature = "serde",
  derive(Serialize, Deserialize),
  serde(try_from = "de::TimeRangeRepr")
)]
#[cfg_attr(
  feature = "quickcheck",
  derive(::quickcheck_richderive::Arbitrary),
  quickcheck(arbitrary = "crate::quickcheck_impls::time_range")
)]
pub struct TimeRange {
  start: i64,
  end: i64,
  timebase: Timebase,
}

impl TimeRange {
  /// Creates a new `TimeRange` with the given start/end PTS and shared timebase.
  ///
  /// # Panics
  ///
  /// - Panics if `end < start` (negative duration).
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn new(start: i64, end: i64, timebase: Timebase) -> Self {
    assert!(start <= end, "end must not precede start");

    Self {
      start,
      end,
      timebase,
    }
  }

  /// Bypass-invariant constructor used only by the `buffa` decode path.
  ///
  /// During protobuf field-by-field merging, intermediate states may
  /// temporarily violate `start <= end` (e.g. `start` field arrives before
  /// `end`, so the partially-decoded struct holds `start=100, end=0`).
  /// The normal `new()` constructor panics in that case. This constructor
  /// skips the assertion so decode can proceed.
  ///
  /// The *final* value is consistent only when the peer is this crate's own
  /// encoder, which never writes `start > end`. A foreign or hostile peer
  /// can write one, and nothing downstream of the last `merge_field` call
  /// re-checks — so a decoded range can violate the invariant, and
  /// [`Self::duration`] then panics on it. Closing that needs a policy this
  /// decoder does not have yet: its other malformed-input arms *clamp* to
  /// stay total (see `buffa.rs`), and there is no obvious clamp for an
  /// inverted range.
  #[cfg(feature = "buffa")]
  #[inline(always)]
  pub(crate) const fn new_for_decode(start: i64, end: i64, timebase: Timebase) -> Self {
    Self {
      start,
      end,
      timebase,
    }
  }

  /// Fallible variant of [`Self::new`]: returns `None` if `end < start`
  /// instead of panicking. Accepts `start == end` (degenerate instant range).
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn try_new(start: i64, end: i64, timebase: Timebase) -> Option<Self> {
    if start <= end {
      Some(Self {
        start,
        end,
        timebase,
      })
    } else {
      None
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

  /// Sets the shared timebase.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn with_timebase(mut self, timebase: Timebase) -> Self {
    self.set_timebase(timebase);
    self
  }

  /// Sets the shared timebase in place.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn set_timebase(&mut self, timebase: Timebase) -> &mut Self {
    self.timebase = timebase;
    self
  }

  /// Returns `true` if `start == end` (a degenerate instant range).
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn is_instant(&self) -> bool {
    self.start == self.end
  }

  /// Returns the span in PTS units (`end - start`) in this timebase.
  ///
  /// Always non-negative given the `start <= end` constructor invariant.
  /// Saturates at `i64::MAX` in the pathological case where `end - start`
  /// would overflow `i64` (e.g., `start = i64::MIN`, `end = i64::MAX`).
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn total_pts(&self) -> i64 {
    self.end.saturating_sub(self.start)
  }

  /// Returns the elapsed [`Duration`] from `start` to `end`.
  ///
  /// # Panics
  ///
  /// Panics if `end` precedes `start`, which every constructor refuses and
  /// [`Self::rescale_to`] preserves — so this is unreachable for a range
  /// built through the public API. It is reachable through the `buffa`
  /// decoder, which admits an inverted range from the wire.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn duration(&self) -> Duration {
    self
      .end()
      .duration_since(&self.start())
      .expect("end must not precede start")
  }

  /// Returns a new `TimeRange` representing the same span in a different timebase.
  ///
  /// Rescales both endpoints via [`Timebase::saturating_rescale`], to the
  /// nearest tick of `target`; round-tripping through a coarser timebase can
  /// lose precision. Because rescaling is monotonic, the `start <= end`
  /// invariant is preserved.
  ///
  /// # Panics
  ///
  /// Panics if `target.num() == 0`, as [`Timebase::saturating_rescale`] does.
  #[cfg_attr(not(tarpaulin), inline(always))]
  pub const fn rescale_to(self, target: Timebase) -> Self {
    Self {
      start: self.timebase.saturating_rescale(self.start, target),
      end: self.timebase.saturating_rescale(self.end, target),
      timebase: target,
    }
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

/// Writes both endpoints as clocks inside interval notation:
/// `[0:00:01.500, 0:00:03.250)`.
///
/// The mismatched brackets are the point rather than decoration. This type is
/// half-open — closed at `start`, open at `end` — and `[…)` is the notation
/// that says so, where a dash or an ellipsis would leave a reader to guess
/// whether `end` is inside. The rendering therefore teaches the semantics the
/// type documents.
///
/// `{:#}` prints the raw endpoints and names the shared timebase **once**,
/// after both — `[1500, 3250) @ 1/1000` — because both endpoints are in one
/// timebase by construction and repeating it would suggest they need not be.
/// The derived [`Debug`] is exact as well, and `{:#}` is the form
/// [`FromStr`](core::str::FromStr) reads back.
///
/// Each endpoint is rendered by [`Timestamp`]'s `Display`, and inherits its
/// truncation, its lossiness, and its indifference to width and alignment
/// flags.
impl fmt::Display for TimeRange {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if f.alternate() {
      return write!(f, "[{}, {}) @ {}", self.start, self.end, self.timebase);
    }
    f.write_str("[")?;
    write_clock(f, self.start, self.timebase)?;
    f.write_str(", ")?;
    write_clock(f, self.end, self.timebase)?;
    f.write_str(")")
  }
}

/// Validators keeping `Deserialize` from being a second construction path.
///
/// A derived `Deserialize` assigns fields directly, so every invariant the
/// constructors enforce has to be re-enforced here or it is not enforced at
/// all: an inbound payload would otherwise mint values the constructors
/// reject, and the arithmetic assumes those are unreachable.
///
/// [`Timebase`]'s two invariants are independent per field, so a
/// `deserialize_with` on each is enough — no intermediate representation and
/// no allocation. (While the fields were `u32`/`NonZeroU32` their types made
/// the violations unrepresentable; `i32`/`NonZeroI32` no longer do.)
/// [`TimeRange`]'s `start <= end` relates two fields, which no per-field hook
/// can see, so that one needs the whole struct in hand first.
#[cfg(feature = "serde")]
mod de {
  use core::{fmt, num::NonZeroI32};
  use serde::{Deserialize, Deserializer, de::Error};

  use crate::{TimeRange, Timebase};

  pub(super) fn de_num<'de, D: Deserializer<'de>>(d: D) -> Result<i32, D::Error> {
    let v = i32::deserialize(d)?;
    if v < 0 {
      return Err(D::Error::custom("timebase numerator must not be negative"));
    }
    Ok(v)
  }

  pub(super) fn de_den<'de, D: Deserializer<'de>>(d: D) -> Result<NonZeroI32, D::Error> {
    let v = NonZeroI32::deserialize(d)?;
    if v.get() < 0 {
      return Err(D::Error::custom("timebase denominator must be positive"));
    }
    Ok(v)
  }

  /// The wire shape of a [`TimeRange`], deserialized before the endpoint
  /// order is checked.
  ///
  /// Field names and their required-ness are the compatibility surface and
  /// match the `Serialize` half exactly; only the check is added.
  #[derive(Deserialize)]
  pub(super) struct TimeRangeRepr {
    start: i64,
    end: i64,
    timebase: Timebase,
  }

  /// A [`TimeRange`] arrived with its endpoints in the wrong order.
  pub(super) struct InvertedRange;

  impl fmt::Display for InvertedRange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      f.write_str("time range end must not precede start")
    }
  }

  impl TryFrom<TimeRangeRepr> for TimeRange {
    type Error = InvertedRange;

    fn try_from(repr: TimeRangeRepr) -> Result<Self, Self::Error> {
      Self::try_new(repr.start, repr.end, repr.timebase).ok_or(InvertedRange)
    }
  }
}

#[cfg(feature = "serde")]
use de::{de_den, de_num};

/// The [well-known roster](Timebase#the-well-known-roster) as one table.
///
/// [`Timebase::from_name`] reads it forward and [`Timebase::well_known_name`]
/// reads it backward, so a constant listed here is reachable from both
/// directions or from neither — there is no second table to forget. The
/// backward direction is single-valued only because no two entries are equal;
/// `well_known_timebases_are_pairwise_distinct` pins that.
const WELL_KNOWN: &[(&str, Timebase)] = &[
  ("SECONDS", Timebase::SECONDS),
  ("MILLIS", Timebase::MILLIS),
  ("MICROS", Timebase::MICROS),
  ("NANOS", Timebase::NANOS),
  ("MPEG_90K", Timebase::MPEG_90K),
  ("HZ_48K", Timebase::HZ_48K),
  ("HZ_44_1K", Timebase::HZ_44_1K),
  ("NTSC_FILM", Timebase::NTSC_FILM),
  ("NTSC_VIDEO", Timebase::NTSC_VIDEO),
  ("FILM_24", Timebase::FILM_24),
  ("PAL_25", Timebase::PAL_25),
];

/// The exact quotient of a rescale, in `i128` and rounded, before either rung
/// of the ladder decides what to do with one that does not fit an `i64`.
///
/// The caller must have ruled out `to.num == 0`; with that out of the way the
/// divisor is strictly positive, because `den > 0` is a constructor invariant
/// on both timebases — which is the whole of the sign analysis this ladder
/// needs, the numerator carrying `pts`'s sign alone.
#[cfg_attr(not(tarpaulin), inline(always))]
const fn rescaled(pts: i64, from: Timebase, to: Timebase) -> i128 {
  // pts * (from.num / from.den) / (to.num / to.den)
  // = pts * from.num * to.den / (from.den * to.num)
  let numerator = (pts as i128) * (from.num as i128) * (to.den.get() as i128);
  let denominator = (from.den.get() as i128) * (to.num as i128);
  div_round_half_away(numerator, denominator)
}

/// Integer division rounding to nearest, halfway cases **away from zero** —
/// the posture FFmpeg's `av_rescale` and `av_rescale_q` take by default
/// (`AV_ROUND_NEAR_INF`), which is why this crate takes it: a PTS rescaled
/// here and by the C library must land on the same tick.
///
/// `d` must be strictly positive, so `n` alone carries the sign and `%` (which
/// takes the sign of the dividend) hands back a remainder whose sign says
/// which way "away from zero" points.
///
/// Doubling the remainder rather than halving the divisor is what makes the
/// halfway test exact for an odd divisor. It cannot overflow at any input this
/// crate can form: `|r| < d`, and `d` is a product of two `i32`s, so `2·|r|`
/// stays under `2^63`.
#[cfg_attr(not(tarpaulin), inline(always))]
const fn div_round_half_away(n: i128, d: i128) -> i128 {
  let q = n / d;
  let r = n % d;
  if r > 0 && 2 * r >= d {
    q + 1
  } else if r < 0 && -2 * r >= d {
    q - 1
  } else {
    q
  }
}

/// [`div_round_half_away`] where both operands are known non-negative, so
/// "away from zero" is "up" and the sign analysis disappears.
///
/// `d` must be non-zero. `2·r` cannot overflow: `r < d`, and every `d` formed
/// here is a product of an `i32` with at most `10^9`, so it stays under
/// `2^62`.
#[cfg_attr(not(tarpaulin), inline(always))]
const fn div_round_half_up(n: u128, d: u128) -> u128 {
  let q = n / d;
  let r = n % d;
  if 2 * r >= d { q + 1 } else { q }
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

/// Renders `pts` in units of `timebase` as `H:MM:SS.mmm`, signed as a whole
/// when the instant is before zero. Shared by [`Timestamp`]'s and
/// [`TimeRange`]'s `Display`, which is where the format is documented.
///
/// Writes straight into the [`fmt::Formatter`]: the crate is `no_std` without
/// `alloc`, so there is no intermediate `String` to build the string in.
fn write_clock(f: &mut fmt::Formatter<'_>, pts: i64, timebase: Timebase) -> fmt::Result {
  const MS_PER_SEC: u128 = 1_000;
  const SECS_PER_MIN: u128 = 60;
  const MINS_PER_HOUR: u128 = 60;

  // Promoted to `i128` for the same reason the rescale ladder promotes: the
  // product overflows `i64` long before the operands are unreasonable. The
  // bound is generous — |pts| ≤ 2^63, `num` < 2^31 by the constructor's sign
  // invariant, and the millisecond factor is < 2^10, so the numerator stays
  // under 2^104 against `i128`'s 2^127. Dividing by `den ≥ 1` cannot grow it.
  //
  // One truncating division, toward zero — *not* the ladder's rounding, for
  // the reason the `Display` docs give. `den` is `NonZeroI32`, so a zero
  // numerator (a legal degenerate timebase) collapses every PTS onto zero
  // rather than dividing by zero.
  let total_ms =
    (pts as i128) * (timebase.num as i128) * (MS_PER_SEC as i128) / (timebase.den.get() as i128);

  // `unsigned_abs`, not negation: the magnitude of the most negative value of a
  // signed type is not representable in it, so `-total_ms` would overflow at
  // the bottom of the range — and `Timestamp::new(i64::MIN, …)` is reachable,
  // `i64::MIN` being FFmpeg's `AV_NOPTS_VALUE`. Carrying the sign separately
  // sidesteps the question entirely.
  let negative = total_ms < 0;
  let magnitude_ms = total_ms.unsigned_abs();

  let millis = magnitude_ms % MS_PER_SEC;
  let total_secs = magnitude_ms / MS_PER_SEC;
  let secs = total_secs % SECS_PER_MIN;
  let total_mins = total_secs / SECS_PER_MIN;
  let mins = total_mins % MINS_PER_HOUR;
  let hours = total_mins / MINS_PER_HOUR;

  if negative {
    f.write_str("-")?;
  }
  write!(f, "{hours}:{mins:02}:{secs:02}.{millis:03}")
}

/// `fn(&mut quickcheck::Gen) -> T` helpers consumed by the per-type
/// `#[quickcheck(arbitrary = "…")]` attributes on each type's
/// `quickcheck-richderive::Arbitrary` derive. The derive emits the actual
/// `impl quickcheck::Arbitrary` blocks; these helpers own the bodies and
/// preserve invariants the field-by-field default would otherwise violate
/// (non-zero denom, non-negative pts, well-formed range).
#[cfg(feature = "quickcheck")]
#[cfg_attr(docsrs, doc(cfg(feature = "quickcheck")))]
pub mod quickcheck_impls {
  use crate::{TimeRange, Timebase, Timestamp};
  use core::num::NonZeroI32;
  use quickcheck::{Arbitrary, Gen};

  /// Numerator in `0..=i32::MAX`, denominator in `1..=i32::MAX` — exactly what
  /// [`Timebase::new`] accepts.
  ///
  /// `quickcheck` implements `Arbitrary` only for the *unsigned* `NonZero`
  /// types, so the denominator cannot be drawn at its field type; both halves
  /// are folded from a `u32` draw instead. Folding rather than rejecting keeps
  /// this total for every `Gen`, including one whose size admits only zero.
  pub fn timebase(g: &mut Gen) -> Timebase {
    const MAX: u32 = i32::MAX as u32;
    let num = (u32::arbitrary(g) % (MAX + 1)) as i32;
    let den = (u32::arbitrary(g) % MAX + 1) as i32;
    Timebase::new(num, NonZeroI32::new(den).expect("den is in 1..=i32::MAX"))
  }

  /// Non-negative `pts` + arbitrary `Timebase`.
  pub fn timestamp(g: &mut Gen) -> Timestamp {
    Timestamp::new(non_negative_i64(g), timebase(g))
  }

  /// `[start, end)` with `start <= end`, both non-negative. The previous
  /// hand-written impl reused a single `Gen` draw for the timebase; same
  /// here. When the two endpoints happen to coincide we bump `end` by 1 to
  /// keep the range non-degenerate (matches the original behavior).
  pub fn time_range(g: &mut Gen) -> TimeRange {
    let a = non_negative_i64(g);
    let b = non_negative_i64(g);
    let start = a.min(b);
    let mut end = a.max(b);
    if start == end {
      end = end.saturating_add(1);
    }
    TimeRange::new(start, end, timebase(g))
  }

  fn non_negative_i64(g: &mut Gen) -> i64 {
    loop {
      let d = i64::arbitrary(g);
      if d >= 0 {
        return d;
      }
    }
  }
}

#[cfg(feature = "arbitrary")]
#[cfg_attr(docsrs, doc(cfg(feature = "arbitrary")))]
const _: () = {
  use arbitrary::Arbitrary;

  impl<'a> Arbitrary<'a> for Timebase {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
      // Drawn in range rather than filtered: `Timebase::new` panics outside
      // it, and a fuzz generator must not be able to trip that.
      let den = u.int_in_range(1..=i32::MAX)?;
      let num = u.int_in_range(0..=i32::MAX)?;
      let den = core::num::NonZeroI32::new(den).expect("den is in 1..=i32::MAX");
      Ok(Timebase::new(num, den))
    }
  }

  impl<'a> Arbitrary<'a> for Timestamp {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
      non_negative_i64(u).and_then(|i| u.arbitrary().map(|tb| Self::new(i, tb)))
    }
  }

  impl<'a> Arbitrary<'a> for TimeRange {
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
      let a = non_negative_i64(u)?;
      let b = non_negative_i64(u)?;
      let start = a.min(b);
      let mut end = a.max(b);

      if start == end {
        end = end.saturating_add(1);
      }

      Ok(TimeRange::new(start, end, u.arbitrary()?))
    }
  }

  fn non_negative_i64(u: &mut arbitrary::Unstructured<'_>) -> arbitrary::Result<i64> {
    loop {
      let val = u.arbitrary::<i64>()?;
      if val >= 0 {
        return Ok(val);
      }
    }
  }
};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod property_tests;

#[cfg(all(test, feature = "serde"))]
mod serde_impl_tests;

#[cfg(all(test, feature = "quickcheck"))]
mod quickcheck_arbitrary_tests;

#[cfg(all(test, feature = "arbitrary"))]
mod arbitrary_impl_tests;

#[cfg(feature = "buffa")]
mod buffa;

/// Ancillary module the buffa code generator looks for when an extern-mapped
/// type is used as a message field with view generation enabled. The mediatime
/// types contain only scalars, so each view is the owned type itself.
#[cfg(feature = "buffa")]
#[doc(hidden)]
pub mod __buffa {
  pub mod view {
    // `'a` is required by buffa's extern-view convention; unused here
    // because these mediatime types are `Copy`/owned (nothing borrowed).
    pub type TimebaseView<'a> = crate::Timebase;
    pub type TimeRangeView<'a> = crate::TimeRange;
    pub type TimestampView<'a> = crate::Timestamp;
  }
}
