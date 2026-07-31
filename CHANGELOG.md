# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.10]

### Changed

- Bump `buffa` dependency from `0.8` to `0.9`. buffa 0.9 replaced the
  `BufMut` bound on `Message::write_to` with its new `EncodeSink` trait, so
  the three hand-written `write_to` impls change one parameter type; every
  `BufMut` implementor is an `EncodeSink` through a blanket impl, so callers
  passing `Vec<u8>`/`BytesMut` are unaffected. **The wire format is
  unchanged** — the tag, varint and length-delimited encoders emit the same
  bytes, and bytes written under buffa 0.6/0.7/0.8 still decode. Consumers
  that bump to `buffa 0.9` must also bump their `mediatime` floor to
  `0.1.10` so a single `buffa` version stays in the dependency graph.

## [0.1.8] — 2026-06-02

### Changed

- Bump `buffa` dependency from `0.6` to `0.7`. Pure version bump — the
  buffa 0.6 → 0.7 breakers (`OwnedView::Deref` removal and the
  `use_bytes_type` extension to `map<K, bytes>` values) don't touch
  mediatime, and the `DefaultInstance` + `Message` impls on `Timebase`,
  `Timestamp`, and `TimeRange` carry over byte-for-byte. The wire format
  is unchanged. Consumers (e.g. mediaschema) that bump to `buffa 0.7`
  must also bump their `mediatime` floor to `0.1.8` so a single `buffa`
  version stays in the dependency graph.

## [0.1.5] — April 23, 2026

### Added

- `Timestamp::duration() -> Option<Duration>` — [`Duration`] from PTS zero in
  the timestamp's own timebase. Returns `None` for negative PTS (pre-roll /
  edit-list cases that have no [`Duration`] representation).
- `TimeRange::rescale_to(target: Timebase) -> Self` — rescales both endpoints
  to a new timebase (parallel to `Timestamp::rescale_to`). Monotonic, so the
  `start <= end` invariant is preserved.
- `TimeRange::total_pts() -> i64` — span in PTS units (`end - start`);
  saturates at `i64::MAX` for pathological `i64::MIN..i64::MAX` inputs.
- `TimeRange::try_new(start, end, timebase) -> Option<Self>` — fallible
  counterpart to `TimeRange::new`; returns `None` instead of panicking when
  `end < start`. Degenerate instant ranges (`start == end`) are accepted.

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
