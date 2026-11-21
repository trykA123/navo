//! Tests for the folder row text helpers: sizes, times, item counts and width modes.

use chrono::{TimeZone, Utc};

use super::folders_text::{Meta, items_text, size_text, time_text};

/// A fixed "now" in 2026 for the time tests.
fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 16, 12, 0, 0).unwrap()
}

#[test]
fn sizes_use_base_1024_with_one_decimal() {
    assert_eq!(size_text(0), "0 B");
    assert_eq!(size_text(512), "512 B");
    assert_eq!(size_text(1023), "1023 B");
    assert_eq!(size_text(1024), "1.0 KB");
    assert_eq!(size_text(3174), "3.1 KB");
    assert_eq!(size_text(24_474), "23.9 KB");
    assert_eq!(size_text(1024 * 1024), "1.0 MB");
    assert_eq!(size_text(2_576_980_378), "2.4 GB");
}

#[test]
fn times_drop_the_year_only_in_the_current_one() {
    let this_year = Utc
        .with_ymd_and_hms(2026, 10, 19, 18, 22, 0)
        .unwrap()
        .timestamp_millis();
    let older = Utc
        .with_ymd_and_hms(2024, 10, 19, 18, 22, 0)
        .unwrap()
        .timestamp_millis();
    assert_eq!(time_text(this_year, &now()), "19 Oct 18:22");
    assert_eq!(time_text(older, &now()), "19 Oct 2024");
    assert_eq!(time_text(i64::MAX, &now()), "");
}

#[test]
fn item_counts_pluralise() {
    assert_eq!(items_text(1), "1 item");
    assert_eq!(items_text(0), "0 items");
    assert_eq!(items_text(2), "2 items");
}

#[test]
fn widths_pick_the_meta_mode() {
    assert_eq!(Meta::for_width(900.0), Meta::Full);
    assert_eq!(Meta::for_width(860.0), Meta::Full);
    assert_eq!(Meta::for_width(859.9), Meta::Size);
    assert_eq!(Meta::for_width(600.0), Meta::Size);
    assert_eq!(Meta::for_width(599.9), Meta::Bare);
}
