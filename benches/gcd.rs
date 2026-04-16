//! Criterion benchmark for the three private GCD helpers used by
//! `Timebase::hash` and `Timestamp::hash`. The functions are copied inline
//! below so the bench doesn't need to expose the private helpers from the
//! crate — keep them bit-identical to `src/lib.rs`.
//!
//! Run with `cargo bench --bench gcd`.

use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

// ---------------------------------------------------------------------------
// Copies of the three GCD implementations from src/lib.rs. Keep in sync.
// ---------------------------------------------------------------------------

#[inline(always)]
fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
  while b != 0 {
    let t = b;
    b = a % b;
    a = t;
  }
  a
}

#[inline(always)]
fn gcd_u128(mut a: u128, mut b: u128) -> u128 {
  while b != 0 {
    let t = b;
    b = a % b;
    a = t;
  }
  a
}

#[inline(always)]
fn binary_gcd_u128(mut a: u128, mut b: u128) -> u128 {
  if a == 0 {
    return b;
  }
  if b == 0 {
    return a;
  }
  let shift = (a | b).trailing_zeros();
  a >>= a.trailing_zeros();
  loop {
    b >>= b.trailing_zeros();
    if a > b {
      core::mem::swap(&mut a, &mut b);
    }
    b -= a;
    if b == 0 {
      return a << shift;
    }
  }
}

// Also benchmark a u32 binary-GCD so the reader can judge whether it's
// worth introducing for Timebase::hash too.
#[inline(always)]
fn binary_gcd_u32(mut a: u32, mut b: u32) -> u32 {
  if a == 0 {
    return b;
  }
  if b == 0 {
    return a;
  }
  let shift = (a | b).trailing_zeros();
  a >>= a.trailing_zeros();
  loop {
    b >>= b.trailing_zeros();
    if a > b {
      core::mem::swap(&mut a, &mut b);
    }
    b -= a;
    if b == 0 {
      return a << shift;
    }
  }
}

// ---------------------------------------------------------------------------
// Workloads
// ---------------------------------------------------------------------------

/// Realistic `Timebase::hash` inputs — `(num, den)` for the common media
/// timebases and frame rates.
fn timebase_u32_inputs() -> &'static [(&'static str, u32, u32)] {
  &[
    ("ms_1/1000", 1, 1000),
    ("ntsc_30000/1001", 30_000, 1001),
    ("mpegts_1/90000", 1, 90_000),
    ("audio_1/48000", 1, 48_000),
    ("fps30_30/1", 30, 1),
    ("coprime_large", 999_983, 999_979), // two large primes
  ]
}

/// Realistic `Timestamp::hash` inputs — `(|pts| * num, den)` as u128, mixing
/// small/large PTS values with typical denominators.
fn timestamp_u128_inputs() -> &'static [(&'static str, u128, u128)] {
  &[
    // 1-second @ 1/1000 → numerator = 1000 * 1 = 1000, den = 1000
    ("1s_ms", 1_000, 1_000),
    // 1-hour @ 1/90000 → num = 90000 * 3600, den = 90000
    ("1h_mpegts", 90_000u128 * 3600, 90_000),
    // NTSC: 30 frames ticks → pts=30, tb=30000/1001 → n=30*30000, d=1001
    ("30fr_ntsc", 30u128 * 30_000, 1001),
    // Large PTS: 10⁹ in MPEG-TS → n = 1_000_000_000 * 1, d = 90_000
    ("big_pts", 1_000_000_000, 90_000),
    // Adversarial: two coprime 64-bit primes packed into u128
    (
      "coprime_64b",
      18_446_744_073_709_551_557u128,
      18_446_744_073_709_551_533u128,
    ),
    // Adversarial: two coprime 96-ish-bit values
    (
      "coprime_96b",
      ((1u128 << 95) | 0x5151_5151_5151_5151_5151_5151u128) | 1,
      ((1u128 << 95) | 0xa5a5_a5a5_a5a5_a5a5_a5a5_a5a5u128) | 3,
    ),
  ]
}

// ---------------------------------------------------------------------------
// Benches
// ---------------------------------------------------------------------------

fn bench_u32(c: &mut Criterion) {
  let mut group = c.benchmark_group("gcd_u32");
  for &(label, a, b) in timebase_u32_inputs() {
    group.bench_with_input(
      BenchmarkId::new("euclidean", label),
      &(a, b),
      |bn, &(a, b)| {
        bn.iter(|| gcd_u32(black_box(a), black_box(b)));
      },
    );
    group.bench_with_input(BenchmarkId::new("binary", label), &(a, b), |bn, &(a, b)| {
      bn.iter(|| binary_gcd_u32(black_box(a), black_box(b)));
    });
  }
  group.finish();
}

fn bench_u128(c: &mut Criterion) {
  let mut group = c.benchmark_group("gcd_u128");
  for &(label, a, b) in timestamp_u128_inputs() {
    group.bench_with_input(
      BenchmarkId::new("euclidean", label),
      &(a, b),
      |bn, &(a, b)| {
        bn.iter(|| gcd_u128(black_box(a), black_box(b)));
      },
    );
    group.bench_with_input(BenchmarkId::new("binary", label), &(a, b), |bn, &(a, b)| {
      bn.iter(|| binary_gcd_u128(black_box(a), black_box(b)));
    });
  }
  group.finish();
}

criterion_group!(benches, bench_u32, bench_u128);
criterion_main!(benches);
