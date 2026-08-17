//! [`FromStr`] for the five time types, and the errors they reject with.
//!
//! Each impl is the inverse of the type's **exact** rendering — the one
//! `{:#}` writes, which is `{}` as well for [`Timebase`], [`Rate`] and
//! [`SignedDuration`], whose renderings have nothing to expand into. That is
//! the only rendering an inverse can exist for: [`Timestamp`]'s and
//! [`TimeRange`]'s default `{}` form is a clock truncated to milliseconds that
//! never names a timebase, so two different instants can share one rendering
//! and no parser can tell which was meant. Accepting it would mint a value
//! that does not compare equal to the one printed. See each impl for its
//! grammar.
//!
//! Each type rejects with **its own** error, named for the vocabulary it
//! wanted: a caller matching on a failed `Rate` parse should not have to read
//! a message about timebases, and the two rosters are disjoint on purpose.

use core::{fmt, num::NonZeroI32, str::FromStr};

use crate::{Rate, SignedDuration, TimeRange, Timebase, Timestamp};

/// The `num/den` half of the two rational grammars, scanned once so
/// [`Timebase`] and [`Rate`] cannot drift apart in what they accept.
///
/// Only the shape is decided here. The caller applies its own constructor as
/// the validator, because the invariants land in different types and are
/// reported under different errors.
fn rational(s: &str) -> Option<(i32, NonZeroI32)> {
  let (num, den) = s.split_once('/')?;
  let num = num.trim().parse::<i32>().ok()?;
  let den = den.trim().parse::<i32>().ok()?;
  Some((num, NonZeroI32::new(den)?))
}

/// Returned when a string is not a [`Timebase`] rendering.
///
/// Carries no detail: the grammar is two integers and a slash, or a name from
/// a fixed roster, so the input is its own diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseTimebaseError(());

impl fmt::Display for ParseTimebaseError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("expected a timebase `num/den` with num >= 0 and den > 0, or a well-known name")
  }
}

impl core::error::Error for ParseTimebaseError {}

/// Returned when a string is not a [`Timestamp`] rendering.
///
/// Also returned for the readable `H:MM:SS.mmm` clock form, which is lossy
/// and therefore not parsed — see the [module docs](self).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseTimestampError(());

impl fmt::Display for ParseTimestampError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("expected a timestamp `pts @ num/den`")
  }
}

impl core::error::Error for ParseTimestampError {}

/// Returned when a string is not a [`SignedDuration`] rendering.
///
/// Distinct from [`ParseTimestampError`] although the two grammars are the
/// same shape: a count and an instant are different vocabularies, and the
/// message says which one was expected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseSignedDurationError(());

impl fmt::Display for ParseSignedDurationError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("expected a signed duration `ticks @ num/den`")
  }
}

impl core::error::Error for ParseSignedDurationError {}

/// Returned when a string is not a [`Rate`] rendering.
///
/// Carries no detail, as [`ParseTimebaseError`] does not: the grammar is two
/// integers and a slash, or a name from a fixed roster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseRateError(());

impl fmt::Display for ParseRateError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("expected a rate `num/den` with num >= 0 and den > 0, or a well-known rate name")
  }
}

impl core::error::Error for ParseRateError {}

/// Returned when a string is not a [`TimeRange`] rendering.
///
/// Also returned when the endpoints parse but run backwards, which
/// [`TimeRange::try_new`] rejects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseTimeRangeError(());

impl fmt::Display for ParseTimeRangeError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("expected a time range `[start, end) @ num/den`, with start <= end")
  }
}

impl core::error::Error for ParseTimeRangeError {}

/// Parses either a [well-known name](Timebase#the-well-known-roster) —
/// `MILLIS`, `MPEG_90K` — or `num/den`, the form [`Timebase`]'s `Display`
/// writes in both `{}` and `{:#}`.
///
/// The roster is tried first, via [`Timebase::from_name`], so the name arm
/// folds ASCII case as that door does — `millis` parses — and nothing else:
/// no alias, no separator guessing. Nothing in the roster contains a slash, so
/// the two arms cannot collide. It is an *input* convenience for
/// hand-written configuration
/// and command lines: `Display` still writes `num/den` for every value, so the
/// `Display` → `FromStr` round trip is unchanged and lossless. The reverse is
/// deliberately not injective — `"MILLIS"` and `"1/1000"` parse to the same
/// timebase, and [`Timebase::well_known_name`] is where the name goes to be
/// recovered.
///
/// Surrounding and interior whitespace is trimmed, so `1 / 1000` parses; on
/// the `num/den` arm the slash is required. The value is **not** reduced:
/// `2/4` parses to a numerator of 2 over a denominator of 4, which is what was
/// written, and what `Display` will write back.
///
/// [`Timestamp`], [`TimeRange`] and [`SignedDuration`] parse their timebase
/// half through this impl, so `12345 @ MPEG_90K` parses too. [`Rate`]'s roster
/// is **not** read here, nor this one there — see that impl for why.
///
/// # Errors
///
/// Returns [`ParseTimebaseError`] if the input is neither a roster name nor a
/// `num/den` pair: the slash is missing, either half is not an `i32`, or the
/// pair is one [`Timebase::try_new`] refuses — a negative numerator, or a
/// denominator that is zero or negative.
impl FromStr for Timebase {
  type Err = ParseTimebaseError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let s = s.trim();
    if let Some(known) = Self::from_name(s) {
      return Ok(known);
    }
    rational(s)
      .and_then(|(num, den)| Self::try_new(num, den))
      .ok_or(ParseTimebaseError(()))
  }
}

/// Parses either a [well-known rate name](Rate#the-well-known-roster) —
/// `FPS_29_97`, `FPS_24` — or `num/den`, the form [`Rate`]'s `Display` writes
/// in both `{}` and `{:#}`.
///
/// The two arms and their order are [`Timebase`]'s, over the *rate* roster:
/// the name arm is tried first, through [`Rate::from_name`], so it folds
/// ASCII case — `fps_29_97` parses — and nothing else. No rate name contains a
/// slash, so the arms cannot collide.
///
/// The rosters, though, are **disjoint on purpose**: `"MILLIS"` is not a rate
/// and `"FPS_24"` is not a timebase, and each door refuses the other's names.
/// A rate and a timebase are reciprocal readings of one rational, so a door
/// that read both would silently answer `1/24` where `24/1` was written.
/// [`Rate::to_timebase`] is the conversion, and it is asked for.
///
/// Whitespace is trimmed as it is on the timebase door, the value is not
/// reduced, and the name arm is an input convenience only: `Display` writes
/// `num/den` for every value, so the `Display` → `FromStr` round trip is
/// lossless and `"FPS_24"` and `"24/1"` land on the same rate.
///
/// # Errors
///
/// Returns [`ParseRateError`] if the input is neither a rate name nor a
/// `num/den` pair: the slash is missing, either half is not an `i32`, or the
/// pair is one [`Rate::try_fps`] refuses — a negative numerator, or a
/// denominator that is zero or negative.
impl FromStr for Rate {
  type Err = ParseRateError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let s = s.trim();
    if let Some(known) = Self::from_name(s) {
      return Ok(known);
    }
    rational(s)
      .and_then(|(num, den)| Self::try_fps(num, den))
      .ok_or(ParseRateError(()))
  }
}

/// Parses `pts @ num/den` — the form [`Timestamp`]'s `Display` writes under
/// `{:#}`.
///
/// Whitespace around each part is trimmed, so `12345@1/90000` parses as well
/// as `12345 @ 1/90000`. The default `{}` clock is **not** accepted: it is
/// truncated to milliseconds and names no timebase, so it cannot name back
/// the instant it was printed from.
///
/// # Errors
///
/// Returns [`ParseTimestampError`] if the `@` is missing, if the PTS is not
/// an `i64`, or if the timebase half is not one [`Timebase`] accepts.
impl FromStr for Timestamp {
  type Err = ParseTimestampError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let err = ParseTimestampError(());
    let (pts, timebase) = s.split_once('@').ok_or(err)?;
    let pts = pts.trim().parse::<i64>().map_err(|_| err)?;
    let timebase = timebase.trim().parse::<Timebase>().map_err(|_| err)?;
    Ok(Self::new(pts, timebase))
  }
}

/// Parses `ticks @ num/den` — the form [`SignedDuration`]'s `Display` writes,
/// under both `{}` and `{:#}`.
///
/// [`Timestamp`]'s grammar over a count: whitespace around each part is
/// trimmed, the timebase half goes through [`Timebase`]'s own impl, so
/// `-1500 @ MILLIS` parses, and the leading `-` is the count's, a timebase
/// having no sign to write.
///
/// The rendering is the same shape as [`Timestamp`]'s `{:#}`, so a string
/// alone does not say which type was printed; the type asked for decides, and
/// `"1500 @ 1/1000".parse::<SignedDuration>()` is a span however the string
/// was produced.
///
/// # Errors
///
/// Returns [`ParseSignedDurationError`] if the `@` is missing, if the count is
/// not an `i64`, or if the timebase half is not one [`Timebase`] accepts.
impl FromStr for SignedDuration {
  type Err = ParseSignedDurationError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let err = ParseSignedDurationError(());
    let (ticks, timebase) = s.split_once('@').ok_or(err)?;
    let ticks = ticks.trim().parse::<i64>().map_err(|_| err)?;
    let timebase = timebase.trim().parse::<Timebase>().map_err(|_| err)?;
    Ok(Self::new(ticks, timebase))
  }
}

/// Parses `[start, end) @ num/den` — the form [`TimeRange`]'s `Display`
/// writes under `{:#}`.
///
/// The half-open brackets are required, in that asymmetry, because they are
/// what the rendering means. Whitespace around each part is trimmed. The
/// default `{}` form, a pair of clocks, is **not** accepted, for the reason
/// [`Timestamp`]'s is not.
///
/// # Errors
///
/// Returns [`ParseTimeRangeError`] if the bracket, comma, or `@` is missing,
/// if an endpoint is not an `i64`, if the timebase half is not one
/// [`Timebase`] accepts, or if the endpoints run backwards.
impl FromStr for TimeRange {
  type Err = ParseTimeRangeError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let err = ParseTimeRangeError(());
    let body = s.trim().strip_prefix('[').ok_or(err)?;
    let (endpoints, tail) = body.split_once(')').ok_or(err)?;
    let (start, end) = endpoints.split_once(',').ok_or(err)?;
    let start = start.trim().parse::<i64>().map_err(|_| err)?;
    let end = end.trim().parse::<i64>().map_err(|_| err)?;
    let timebase = tail
      .trim()
      .strip_prefix('@')
      .ok_or(err)?
      .trim()
      .parse::<Timebase>()
      .map_err(|_| err)?;
    Self::try_new(start, end, timebase).ok_or(err)
  }
}

#[cfg(test)]
mod tests;
