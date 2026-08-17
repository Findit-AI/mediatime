use super::*;

use crate::{WELL_KNOWN, nz};

/// The derived `Debug` prints every field, which makes it the *structural*
/// comparison this crate's `PartialEq` deliberately is not: `2/4 == 1/2`
/// under `==`, so `==` alone would not catch a parser that silently reduced.
fn assert_same_fields<T: fmt::Debug>(parsed: &T, original: &T) {
  assert_eq!(format!("{parsed:?}"), format!("{original:?}"));
}

fn timebases() -> [Timebase; 7] {
  [
    Timebase::new(1, nz(1000)),
    Timebase::new(1, nz(90_000)),
    Timebase::new(30_000, nz(1001)),
    Timebase::new(0, nz(3)),
    Timebase::new(2, nz(4)),
    Timebase::default(),
    Timebase::new(i32::MAX, nz(i32::MAX)),
  ]
}

#[test]
fn timebase_round_trips_through_display() {
  for tb in timebases() {
    // `{}` and `{:#}` are the same rendering for this type, so both are
    // inverses.
    for rendered in [format!("{tb}"), format!("{tb:#}")] {
      let parsed: Timebase = rendered.parse().expect("Display output parses back");
      assert_eq!(parsed, tb);
      assert_same_fields(&parsed, &tb);
    }
  }
}

#[test]
fn timebase_parse_does_not_reduce() {
  // `Display` writes the declared form rather than the reduced one, so the
  // inverse must keep the declared form too — a parser that reduced would
  // still satisfy `==`.
  let parsed: Timebase = "2/4".parse().expect("parses");
  assert_eq!(parsed.num(), 2);
  assert_eq!(parsed.den().get(), 4);
}

#[test]
fn timebase_parse_trims_whitespace() {
  for s in ["1/1000", " 1/1000", "1/1000 ", "1 / 1000", "  1  /  1000  "] {
    assert_same_fields(
      &s.parse::<Timebase>().expect("parses"),
      &Timebase::new(1, nz(1000)),
    );
  }
}

#[test]
fn timebase_parse_rejects_what_the_constructor_rejects() {
  // The constructor's sign invariants are what the arithmetic assumes, so
  // parsing must not be a second way in.
  for s in ["-1/1000", "1/0", "1/-1000", "-1/-1"] {
    assert_eq!(s.parse::<Timebase>(), Err(ParseTimebaseError(())), "{s}");
  }
}

#[test]
fn timebase_parses_a_well_known_name_on_its_other_arm() {
  // The roster arm is tried first; `Display` is unchanged, so the round trip
  // it inverts still goes through `num/den`.
  for (name, expected) in WELL_KNOWN {
    assert_same_fields(&name.parse::<Timebase>().expect("parses"), expected);
    assert_eq!(
      format!("{expected}"),
      format!("{}/{}", expected.num(), expected.den())
    );
  }

  // Whitespace is trimmed around a name as it is around a rational.
  assert_same_fields(
    &"  MPEG_90K  ".parse::<Timebase>().expect("parses"),
    &Timebase::MPEG_90K,
  );

  // The name is an input convenience only: two spellings, one value, and the
  // rendering is always the rational.
  assert_eq!("MILLIS".parse::<Timebase>(), "1/1000".parse::<Timebase>());

  // The composite parsers inherit the arm, since they parse their timebase
  // half through this impl.
  assert_same_fields(
    &"12345 @ MPEG_90K".parse::<Timestamp>().expect("parses"),
    &Timestamp::new(12_345, Timebase::MPEG_90K),
  );
  assert_same_fields(
    &"[100, 500) @ MILLIS".parse::<TimeRange>().expect("parses"),
    &TimeRange::new(100, 500, Timebase::MILLIS),
  );
}

#[test]
fn timebase_parse_rejects_a_name_that_is_not_on_the_roster() {
  // Case-sensitive and exact, per `Timebase::from_name`; nothing falls
  // through to the rational arm, which has no slash to find.
  for s in ["millis", "MILLI", "MPEG90K", "SECOND", "NTSC"] {
    assert_eq!(s.parse::<Timebase>(), Err(ParseTimebaseError(())), "{s}");
  }
}

#[test]
fn timebase_parse_rejects_malformed_input() {
  for s in [
    "",
    "1",
    "/1000",
    "1/",
    "1/2/3",
    "a/b",
    "1.5/2",
    "1/1000 @ 2",
    // Beyond `i32` at either end.
    "2147483648/1",
    "1/2147483648",
  ] {
    assert_eq!(s.parse::<Timebase>(), Err(ParseTimebaseError(())), "{s}");
  }
}

#[test]
fn timestamp_round_trips_through_the_exact_display() {
  for tb in timebases() {
    for pts in [0i64, 1, -1, 12_345, -1500, i64::MAX, i64::MIN] {
      let ts = Timestamp::new(pts, tb);
      let parsed: Timestamp = format!("{ts:#}").parse().expect("`{:#}` parses back");
      assert_eq!(parsed, ts);
      assert_same_fields(&parsed, &ts);
    }
  }
}

#[test]
fn timestamp_parse_rejects_the_clock_form() {
  // The clock is truncated to milliseconds and names no timebase, so it
  // cannot name back the instant it was printed from. Two timestamps that
  // are *not* equal share this rendering.
  let a = Timestamp::new(12_345, Timebase::new(1, nz(90_000)));
  let b = Timestamp::new(137, Timebase::new(1, nz(1000)));
  assert_eq!(format!("{a}"), format!("{b}"));
  assert_ne!(a, b);
  assert_eq!(
    format!("{a}").parse::<Timestamp>(),
    Err(ParseTimestampError(()))
  );
}

#[test]
fn timestamp_parse_trims_whitespace_around_the_separator() {
  let expected = Timestamp::new(12_345, Timebase::new(1, nz(90_000)));
  for s in ["12345 @ 1/90000", "12345@1/90000", "  12345  @  1/90000  "] {
    assert_same_fields(&s.parse::<Timestamp>().expect("parses"), &expected);
  }
}

#[test]
fn timestamp_parse_rejects_malformed_input() {
  for s in [
    "",
    "12345",
    "@1/1000",
    "12345 @",
    "12345 @ 1/1000 @ 2",
    "abc @ 1/1000",
    "12345 @ -1/1000",
    "0:00:00.137",
  ] {
    assert_eq!(s.parse::<Timestamp>(), Err(ParseTimestampError(())), "{s}");
  }
}

#[test]
fn time_range_round_trips_through_the_exact_display() {
  let ms = Timebase::new(1, nz(1000));
  for range in [
    TimeRange::new(1500, 3250, ms),
    TimeRange::new(-1500, 3250, ms),
    TimeRange::new(0, 0, Timebase::default()),
    TimeRange::new(i64::MIN, i64::MAX, Timebase::new(30_000, nz(1001))),
    TimeRange::instant(Timestamp::new(12_345, Timebase::new(1, nz(90_000)))),
  ] {
    let parsed: TimeRange = format!("{range:#}").parse().expect("`{:#}` parses back");
    assert_eq!(parsed, range);
    assert_same_fields(&parsed, &range);
  }
}

#[test]
fn time_range_parse_rejects_the_clock_form() {
  let range = TimeRange::new(1500, 3250, Timebase::new(1, nz(1000)));
  assert_eq!(
    format!("{range}").parse::<TimeRange>(),
    Err(ParseTimeRangeError(()))
  );
}

#[test]
fn time_range_parse_trims_whitespace() {
  let expected = TimeRange::new(1500, 3250, Timebase::new(1, nz(1000)));
  for s in [
    "[1500, 3250) @ 1/1000",
    "[1500,3250)@1/1000",
    "  [ 1500 , 3250 ) @ 1 / 1000  ",
  ] {
    assert_same_fields(&s.parse::<TimeRange>().expect("parses"), &expected);
  }
}

#[test]
fn time_range_parse_rejects_backwards_endpoints() {
  // `TimeRange::new` panics on these and `try_new` returns `None`; parsing
  // must not be a third path that admits them.
  assert_eq!(
    "[3250, 1500) @ 1/1000".parse::<TimeRange>(),
    Err(ParseTimeRangeError(()))
  );
}

#[test]
fn time_range_parse_rejects_malformed_input() {
  for s in [
    "",
    "1500, 3250) @ 1/1000",
    "[1500, 3250 @ 1/1000",
    "[1500 3250) @ 1/1000",
    "[1500, 3250)",
    "[1500, 3250) 1/1000",
    "[1500, 3250) @ 1/0",
    "[1500, 3250, 4000) @ 1/1000",
    "(1500, 3250) @ 1/1000",
  ] {
    assert_eq!(s.parse::<TimeRange>(), Err(ParseTimeRangeError(())), "{s}");
  }
}

#[test]
fn parse_errors_name_the_grammar_they_wanted() {
  fn message(e: &dyn core::error::Error) -> String {
    format!("{e}")
  }

  assert_eq!(
    message(&ParseTimebaseError(())),
    "expected a timebase `num/den` with num >= 0 and den > 0, or a well-known name"
  );
  assert_eq!(
    message(&ParseTimestampError(())),
    "expected a timestamp `pts @ num/den`"
  );
  assert_eq!(
    message(&ParseTimeRangeError(())),
    "expected a time range `[start, end) @ num/den`, with start <= end"
  );
}
