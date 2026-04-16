# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] — April 17, 2026

Initial public release. First-cut API — expect minor refinements before 1.0.

### Added

- `Timebase` — rational `num/den` (`u32` numerator, `NonZeroU32` denominator).
  Mirrors FFmpeg's `AVRational`. Supports value-based equality, ordering, and
  hashing (reduced-form rational), so `1/2 == 2/4 == 3/6` and all three hash
  identically.
- `Timestamp` — integer PTS (`i64`) tagged with a `Timebase`. Semantic
  comparison across different timebases via 128-bit cross-multiplication —
  no rounding, no division.
- `TimeRange` — half-open `[start, end)` interval sharing a `Timebase`, with
  `start()` / `end()` as `Timestamp`, `duration()`, and clamped linear
  `interpolate(t)` for midpoint / bias placement.
- Timebase utilities: `rescale_pts` (FFmpeg's `av_rescale_q`), `rescale`,
  `frames_to_duration`, `duration_to_pts`, `num`/`den` accessors, `with_*`
  consuming builders and `set_*` in-place setters.
- Timestamp utilities: `pts`/`timebase` accessors, `with_pts`/`set_pts`,
  `rescale_to`, `saturating_sub_duration`, `duration_since`, `cmp_semantic`
  (const-fn form of `Ord::cmp`).
- TimeRange utilities: `new`, `instant`, `start_pts`/`end_pts`/`timebase`
  accessors, `with_*`/`set_*` setters for both endpoints, `is_instant`.
- `const fn` across the whole public surface — every constructor, accessor,
  and setter can be evaluated in a `const` context.
- `#![no_std]` always, zero dependencies. No allocation anywhere — every
  public type is `Copy`.

### Behavior

- All comparisons between types are **semantic**, not structural: two
  `Timestamp`s representing the same instant in different timebases are
  `Eq`, `Ord::Equal`, and hash the same. Use this directly as a `HashMap` or
  `BTreeMap` key without worrying about canonicalization.
- Cross-timebase arithmetic (rescaling, `duration_since`) uses 128-bit
  intermediates throughout — exact for any `u32`×`u32` timebase combined
  with any `i64` PTS in the real-video range.
- `rescale_pts` rounds toward zero, matching `av_rescale_q` with default
  rounding. Saturating variants are not provided yet — overflow in
  `duration_to_pts` is clamped to `i64::MAX`.

### Testing

- 100% line coverage on `src/lib.rs` under
  `cargo tarpaulin --all-features --run-types tests --run-types doctests`.
- Criterion bench (`cargo bench --bench gcd`) for the internal GCD helpers
  used by `Hash`; ships both Euclidean and binary variants for comparison.
