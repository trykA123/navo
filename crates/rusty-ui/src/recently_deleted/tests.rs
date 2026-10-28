use chrono::{TimeZone, Utc};
use gpui_kit::Modifiers;
use rusty_core::Location;
use rusty_core::retention::RetentionId;

use super::entries::{Day, Entry, Model};
use super::{Event, event_for_key, text};
use crate::browser::{RetainedKind, RetainedRow, RetainedState};

const HOUR: i64 = 3_600_000;

fn at(day: u32, hour: u32) -> i64 {
    Utc.with_ymd_and_hms(2026, 10, day, hour, 0, 0)
        .single()
        .expect("valid time")
        .timestamp_millis()
}

fn row(id: i64, name: &str, deleted_at_ms: i64) -> RetainedRow {
    RetainedRow {
        id: RetentionId(id),
        original: Location::new(format!("/work/{name}")).expect("location"),
        kind: RetainedKind::Delete,
        bytes: 1000,
        deleted_at_ms,
        expires_at_ms: deleted_at_ms + 72 * HOUR,
        state: RetainedState::Ready,
    }
}

fn ids(model: &Model) -> Vec<Option<i64>> {
    model
        .entries()
        .iter()
        .map(|entry| match entry {
            Entry::Day(_) => None,
            Entry::Item(row) => Some(row.id.0),
        })
        .collect()
}

#[test]
fn rows_group_by_day_with_the_newest_day_and_row_first() {
    let rows = [
        row(1, "old.txt", at(25, 9)),
        row(2, "yesterday.txt", at(27, 8)),
        row(3, "early.txt", at(28, 7)),
        row(4, "late.txt", at(28, 11)),
    ];
    let model = Model::new(&rows, at(28, 12), &Utc);
    assert_eq!(
        ids(&model),
        [None, Some(4), Some(3), None, Some(2), None, Some(1)]
    );
    let days: Vec<Day> = model
        .entries()
        .iter()
        .filter_map(|entry| match entry {
            Entry::Day(head) => Some(head.day),
            Entry::Item(_) => None,
        })
        .collect();
    assert_eq!(days[0], Day::Today);
    assert_eq!(days[1], Day::Yesterday);
    assert!(matches!(days[2], Day::On(_)));
}

#[test]
fn a_day_head_counts_its_rows_and_bytes() {
    let rows = [row(1, "a", at(28, 9)), row(2, "b", at(28, 10))];
    let model = Model::new(&rows, at(28, 12), &Utc);
    let Some(Entry::Day(head)) = model.entries().first() else {
        panic!("no day head");
    };
    assert_eq!((head.count, head.bytes), (2, 2000));
    assert_eq!((model.items(), model.bytes()), (2, 2000));
}

#[test]
fn stepping_skips_day_heads_and_stays_at_the_ends() {
    let rows = [row(1, "a", at(27, 9)), row(2, "b", at(28, 9))];
    let model = Model::new(&rows, at(28, 12), &Utc);
    assert_eq!(model.first_item(), Some(1));
    assert_eq!(model.step(1, true), 3);
    assert_eq!(model.step(3, false), 1);
    assert_eq!(model.step(1, false), 1);
    assert_eq!(model.step(3, true), 3);
    assert_eq!(model.step(0, true), 1);
}

#[test]
fn an_empty_model_has_no_cursor() {
    let model = Model::new(&[], at(28, 12), &Utc);
    assert!(model.is_empty());
    assert_eq!(model.first_item(), None);
    assert_eq!(model.step(0, true), 0);
}

#[test]
fn expiry_reads_days_hours_then_minutes() {
    assert_eq!(text::expires(51 * HOUR), "expires in 2d 3h");
    assert_eq!(text::expires(90 * 60_000), "expires in 1h 30m");
    assert_eq!(text::expires(10_000), "expires in 1m");
    assert_eq!(text::expires(0), "expiring now");
}

#[test]
fn keys_map_to_events_only_without_modifiers() {
    let none = Modifiers::default();
    assert_eq!(event_for_key("up", none), Some(Event::Move(false)));
    assert_eq!(event_for_key("down", none), Some(Event::Move(true)));
    assert_eq!(event_for_key("enter", none), Some(Event::Restore));
    assert_eq!(event_for_key("delete", none), Some(Event::Delete));
    assert_eq!(event_for_key("escape", none), Some(Event::Close));
    assert_eq!(event_for_key("a", none), None);
    let ctrl = Modifiers {
        control: true,
        ..none
    };
    assert_eq!(event_for_key("delete", ctrl), None);
}
