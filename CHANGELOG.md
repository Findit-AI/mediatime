# Changelog

All notable changes to this crate are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0]

### Added

- `SignedDuration` — the vector to `Timestamp`'s point: `{ ticks: i64, timebase }`
  with the full named ladder (`checked_`/`saturating_` × `neg`/`abs`/`add`/`sub`),
  `cmp_semantic`, and the `rescale_to`/`checked_rescale_to` pair. Deliberately no
  `Ord`: length ordering is asked for by name — `sort_by(SignedDuration::cmp_semantic)`
  — because the structural order a derive would give sorts `2 @ 1/1` (two seconds)
  ahead of `1000 @ 1/1000` (one second).
- `Rate` — the other reading of a rational, where the value *is* the rate
  (`30000/1001` fps stores `30000/1001`): `hz`/`fps` constructors with `try_`
  twins, the reciprocal layer `to_timebase`/`from_timebase`
  (+ `checked_` twins), eight `FPS_*` constants with `from_name`/
  `well_known_name`, and the frame arithmetic that used to live on `Timebase`:
  `checked_frames_to_duration`/`saturating_frames_to_duration`.
- `Timestamp::signed_duration_since`/`checked_signed_duration_since` and the
  four signed rungs `checked_add_signed`/`saturating_add_signed`/
  `checked_sub_signed`/`saturating_sub_signed`.
- `Display`/`FromStr` for the two new types: `SignedDuration` renders the exact
  form (`-1500 @ 1/1000`), `Rate` the numeric form and additionally parses its
  roster names. Each has its own parse error (`ParseSignedDurationError`,
  `ParseRateError`); the two rosters are deliberately disjoint
  (`"MILLIS".parse::<Rate>()` errors).
- `Display` for `Timebase`, `Timestamp` and `TimeRange`.
- The `Timebase` roster grows to 25 named constants: the audio sample-rate
  family (`HZ_8K` … `HZ_192K`, fourteen members) and the frame-interval family
  completed to eight (`VIDEO_30`, `PAL_50`, `NTSC_60`, `VIDEO_60` join
  `NTSC_FILM`/`NTSC_VIDEO`/`FILM_24`/`PAL_25`), with `from_name`/
  `well_known_name` and a test-pinned bijection against the `Rate` roster. One
  value, one name — no aliases (Matroska's millisecond base is `MILLIS`).
- Name lookups (`from_name`, and `FromStr` where it reads names) accept
  ASCII-case-insensitively; the canonical spelling is what `well_known_name`
  returns.

### Changed

- **Breaking:** the bare-name arithmetic is gone. `Timebase::rescale_pts`,
  `Timebase::rescale` and `Timebase::duration_to_pts` are replaced by the
  named ladder: `checked_rescale`/`saturating_rescale` and
  `checked_duration_to_pts`/`saturating_duration_to_pts` — every lossy
  operation spells its overflow posture.
- **Breaking:** rounding is round-to-nearest, ties away from zero — FFmpeg's
  `AV_ROUND_NEAR_INF`, which is what `av_rescale_q` actually defaults to. The
  old docs claimed truncation matched `av_rescale_q`; they were wrong, and the
  whole ladder (rescale, duration conversion, and `Rate`'s frame arithmetic)
  now rounds one way.
- **Breaking:** a degenerate timebase (`num == 0`) now panics on both
  saturating rungs (`saturating_rescale` already did;
  `saturating_duration_to_pts` answered `0`) — `i64::saturating_div`'s
  precedent: saturation is a posture toward overflow, and a degenerate base
  has no quotient to clamp. `Timestamp::saturating_add_duration`/
  `saturating_sub_duration` panic accordingly instead of silently not moving.
- **Breaking:** `Timestamp`'s semantic comparison guards its same-timebase
  fast path against degenerate timebases. Before the guard, `Eq` was not
  transitive (`1 @ 0/1 == 0 @ 1/1` and `2 @ 0/1 == 0 @ 1/1`, yet
  `1 @ 0/1 != 2 @ 0/1`), which `BTreeMap`/`sort` rely on; every instant of a
  degenerate timebase now compares equal through the cross-multiply, agreeing
  with `Hash`.

## [0.2.0]

### Changed

- **Breaking:** `Timebase`'s numerator and denominator are now signed —
  `num: u32 → i32` and `den: NonZeroU32 → NonZeroI32`. FFmpeg's `AVRational`
  is a pair of C `int`s, so a `u32` numerator or denominator above `i32::MAX`
  was representable but could not round-trip into an `AVRational` — usable in
  Rust, unusable at the boundary with the decoder library this crate exists to
  serve. `i32` is also a native `INTEGER` on PostgreSQL, MySQL and SQLite,
  whereas `sqlx` has no `Type<Postgres>`/`Encode<Postgres>` for `u32` at all.
  `new`, `try_new`, `num`, `den`, `with_num`, `with_den`, `set_num` and
  `set_den` all change signature.
- **Breaking:** `Timebase::new` now panics on `num < 0` or `den < 0`.
  `NonZeroI32` carries only the non-zero half of what `NonZeroU32` guaranteed,
  so the sign half moved into the constructor; the setters route through it so
  there is one enforcement site. A zero numerator remains legal (a degenerate
  timebase, still not a valid rescale target).
- Bump `buffa` dependency from `0.8` to `0.9`. buffa 0.9 replaced the
  `BufMut` bound on `Message::write_to` with its new `EncodeSink` trait, so
  the three hand-written `write_to` impls change one parameter type; every
  `BufMut` implementor is an `EncodeSink` through a blanket impl, so callers
  passing `Vec<u8>`/`BytesMut` are unaffected. Consumers that bump to
  `buffa 0.9` must also bump their `mediatime` floor to `0.2.0` so a single
  `buffa` version stays in the dependency graph.
- Bump `quickcheck-richderive` dependency from `0.3` to `0.4`. The derive's
  0.4.0 is a one-dependency release — `syn 2 → syn 3` — forced by syn 3
  replacing `Signature::unsafety: Option<Token![unsafe]>` with the tri-state
  `Signature::safety` that Rust 2024's `unsafe extern` needs. It changes no
  emitted token and no diagnostic, so the three derived
  `impl quickcheck::Arbitrary` blocks on `Timebase`, `Timestamp` and
  `TimeRange` expand byte-for-byte as before; the bump is confined to the
  optional `quickcheck` feature and moves no public API, no wire format and
  no runtime behaviour. It does drop `syn 2` from the normal dependency
  graph, leaving `syn 3` as the only `syn` a consumer compiles for
  mediatime itself.

### Added

- `Timebase::try_new(num, den) -> Option<Self>` — fallible counterpart to
  `Timebase::new`, mirroring `TimeRange::try_new`.
- `Display` for `Timebase`, `Timestamp` and `TimeRange`, each with a readable
  default form and an exact alternate form under `{:#}`:

  | type | `{}` | `{:#}` |
  |---|---|---|
  | `Timebase` | `1/1000` | `1/1000` |
  | `Timestamp` | `0:00:00.137` | `12345 @ 1/90000` |
  | `TimeRange` | `[0:00:01.500, 0:00:03.250)` | `[1500, 3250) @ 1/1000` |

  `Timebase` prints the form proposed in the request, unreduced — the timebase
  a stream declared is the one worth reading in a log, and `2/4` would
  otherwise be indistinguishable from `1/2`.

  `Timestamp` diverges from `video-rs`, which prints the unreduced rational
  (`12345/90000 secs`) in this position. Readable log messages were the point
  of the request, and that form makes the reader do the division; the rational
  stays available under `{:#}`. Hours are unpadded and unbounded
  (`123:45:06.789`), minutes and seconds are two digits, milliseconds three,
  and a negative PTS signs the whole rendering (`-0:00:01.500`) since pre-roll
  and edit lists produce one. The value is truncated toward zero at
  millisecond resolution, as `rescale_pts` truncates, so `{}` is lossy in both
  precision and timebase — `{:#}` and the derived `Debug` are the exact forms.

  `TimeRange` renders `[…)` because the interval is half-open, so the notation
  carries the semantics the type documents; `{:#}` names the shared timebase
  once, after both endpoints.

  Nothing allocates: the impls write directly into the `Formatter`, so the
  crate remains `no_std` with no `alloc`. One consequence is documented on each
  impl — width and alignment flags (`{:>12}`) are ignored, because honouring
  them means measuring the finished string and there is no buffer to build one
  in.

### Fixed

- Deserializing a `Timebase` can no longer produce a value the constructor
  would reject. serde's derive assigns fields directly, and the field types no
  longer make a negative numerator or denominator unrepresentable, so both
  fields are validated on the way in.

### Wire compatibility

- **The buffa wire format is unchanged.** The two `Timebase` fields move from
  protobuf `uint32` to `int32`, which is the same plain (non-ZigZag) varint for
  every value a `Timebase` can hold; bytes encoded by earlier versions still
  decode to the same value, and a golden-bytes test pins this. Values above
  `i32::MAX` written by an older peer decode to the smallest legal value
  rather than panicking — they were never representable in the new type.
- **The buffa 0.9 encoder swap is byte-for-byte transparent** as well: the tag,
  varint and length-delimited encoders emit the same bytes through `EncodeSink`
  as they did through `BufMut`, and bytes written under buffa 0.6/0.7/0.8 still
  decode.
- **The serde representation is unchanged** for all in-range values: the
  `numerator`/`denominator` field names and their JSON number encoding are
  untouched.

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
