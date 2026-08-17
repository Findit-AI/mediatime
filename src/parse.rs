//! [`FromStr`] for the three time types, and the errors they reject with.
//!
//! Each impl is the inverse of the type's **exact** rendering — the one
//! `{:#}` writes, which is `{}` as well for [`Timebase`]. That is the only
//! rendering an inverse can exist for: [`Timestamp`]'s and [`TimeRange`]'s
//! default `{}` form is a clock truncated to milliseconds that never names a
//! timebase, so two different instants can share one rendering and no parser
//! can tell which was meant. Accepting it would mint a value that does not
//! compare equal to the one printed. See each impl for its grammar.

use core::{fmt, num::NonZeroI32, str::FromStr};

use crate::{TimeRange, Timebase, Timestamp};

/// Returned when a string is not a [`Timebase`] rendering.
///
/// Carries no detail: the grammar is two integers and a slash, so the input
/// is its own diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseTimebaseError(());

impl fmt::Display for ParseTimebaseError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("expected a timebase `num/den`, with num >= 0 and den > 0")
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

/// Parses `num/den` — the form [`Timebase`]'s `Display` writes, in both `{}`
/// and `{:#}`.
///
/// Surrounding and interior whitespace is trimmed, so `1 / 1000` parses; the
/// slash is required. The value is **not** reduced: `2/4` parses to a
/// numerator of 2 over a denominator of 4, which is what was written, and
/// what `Display` will write back.
///
/// # Errors
///
/// Returns [`ParseTimebaseError`] if the slash is missing, if either half is
/// not an `i32`, or if the pair is one [`Timebase::try_new`] refuses — a
/// negative numerator, or a denominator that is zero or negative.
impl FromStr for Timebase {
  type Err = ParseTimebaseError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let err = ParseTimebaseError(());
    let (num, den) = s.split_once('/').ok_or(err)?;
    let num = num.trim().parse::<i32>().map_err(|_| err)?;
    let den = den.trim().parse::<i32>().map_err(|_| err)?;
    NonZeroI32::new(den)
      .and_then(|den| Self::try_new(num, den))
      .ok_or(err)
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
