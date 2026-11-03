use navo_core::CursorMove;

use super::mode::{deliver, root_rows, select, selected, start, visible};
use super::{browser, row, rows, sides};
use crate::browser::compare::view::{look_of, status_text};
use crate::browser::{
    CompareFilter, CompareKind, CompareSide, CompareTotals, CompareUpdate, RowProgress, RowStatus,
    Tone, bar_view, empty_text, pane_rows,
};

const MIB: u64 = 1 << 20;

fn started() -> (crate::browser::Browser, std::sync::Arc<super::Calls>) {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    (browser, calls)
}

fn paths(browser: &crate::browser::Browser, side: usize) -> Vec<String> {
    let mode = browser.compare_mode().expect("compare mode");
    pane_rows(mode, side)
        .iter()
        .map(|row| row.path.clone())
        .collect()
}

#[test]
fn both_panes_list_the_same_rows_in_the_same_order() {
    let (mut browser, calls) = started();
    deliver(&mut browser, &calls, root_rows());
    assert_eq!(paths(&browser, 0), paths(&browser, 1));
    assert_eq!(paths(&browser, 0), visible(&browser));
}

#[test]
fn the_side_without_the_file_gets_a_gap_and_no_status_words() {
    let (mut browser, calls) = started();
    deliver(&mut browser, &calls, root_rows());
    let mode = browser.compare_mode().expect("compare mode");
    let (left, right) = (pane_rows(mode, 0), pane_rows(mode, 1));
    let at = |rows: &[crate::browser::RowView], path: &str| {
        rows.iter().position(|row| row.path == path).expect("row")
    };
    let only_left = at(&left, "b.txt");
    assert!(left[only_left].side.is_some());
    assert_eq!(left[only_left].status, "only left");
    assert!(right[only_left].side.is_none());
    assert_eq!(right[only_left].status, "");
    let same = at(&left, "a.txt");
    assert!(left[same].side.is_some() && right[same].side.is_some());
    assert_eq!(left[same].status, "");
}

#[test]
fn sizes_and_times_come_from_the_side_that_has_the_file() {
    let (mut browser, calls) = started();
    let side = |size| {
        Some(CompareSide {
            kind: CompareKind::File,
            size,
            modified_ms: None,
        })
    };
    let mut changed = row(1, "a.txt", RowStatus::Different);
    (changed.left, changed.right) = (side(2048), side(3 * MIB));
    deliver(&mut browser, &calls, rows(1, vec![changed]));
    let mode = browser.compare_mode().expect("compare mode");
    let sizes: Vec<String> = [0, 1]
        .map(|at| pane_rows(mode, at)[0].side.clone().expect("side").size)
        .into();
    assert_eq!(sizes, ["2.0 KB", "3.0 MB"]);
}

#[test]
fn every_status_has_a_tone_and_a_mark() {
    let cases = [
        (RowStatus::Same, Tone::Muted, "="),
        (RowStatus::SameAfterRules, Tone::Muted, "="),
        (RowStatus::SameSizeTime, Tone::Muted, "="),
        (RowStatus::Different, Tone::Change, "≠"),
        (RowStatus::OnlyLeft, Tone::Delete, "←"),
        (RowStatus::OnlyRight, Tone::Add, "→"),
        (RowStatus::TypeConflict, Tone::Conflict, "!"),
        (RowStatus::CaseConflict, Tone::Conflict, "!"),
        (RowStatus::Checking, Tone::Busy, "…"),
        (
            RowStatus::NotChecked("compare-reason-stopped"),
            Tone::Muted,
            "–",
        ),
        (
            RowStatus::Unavailable("compare-reason-access-denied"),
            Tone::Conflict,
            "!",
        ),
    ];
    for (status, tone, mark) in cases {
        assert_eq!(look_of(status), (tone, mark), "{status:?}");
    }
}

#[test]
fn status_words_follow_the_plan() {
    let words = |status| status_text(status, None);
    assert_eq!(words(RowStatus::Same), "");
    assert_eq!(words(RowStatus::SameAfterRules), "same after rules");
    assert_eq!(words(RowStatus::SameSizeTime), "same size and time");
    assert_eq!(words(RowStatus::Different), "different");
    assert_eq!(words(RowStatus::TypeConflict), "type conflict");
    assert_eq!(words(RowStatus::CaseConflict), "case conflict");
    assert_eq!(words(RowStatus::Checking), "same size and time · checking");
    let stopped = words(RowStatus::NotChecked("compare-reason-stopped"));
    assert_eq!(stopped, "Not checked (stopped)");
    let denied = words(RowStatus::Unavailable("compare-reason-access-denied"));
    assert_eq!(denied, "unavailable: access denied");
}

#[test]
fn a_checking_row_shows_the_bytes_checked_when_progress_is_known() {
    let progress = RowProgress {
        id: 1,
        bytes_done: 100 * MIB,
        bytes_total: 600 * MIB,
    };
    let text = status_text(RowStatus::Checking, Some(progress));
    assert_eq!(text, "same size and time · checking · 100.0 MB of 600.0 MB");
    let empty = RowProgress {
        bytes_total: 0,
        ..progress
    };
    assert_eq!(
        status_text(RowStatus::Checking, Some(empty)),
        "same size and time · checking"
    );
}

#[test]
fn progress_from_the_engine_reaches_the_row() {
    let (mut browser, calls) = started();
    let update = CompareUpdate::Rows {
        generation: 1,
        rows: vec![row(7, "big.txt", RowStatus::Checking)],
        progress: vec![RowProgress {
            id: 7,
            bytes_done: 10 * MIB,
            bytes_total: 100 * MIB,
        }],
    };
    deliver(&mut browser, &calls, update);
    let mode = browser.compare_mode().expect("compare mode");
    let status = pane_rows(mode, 0)[0].status.clone();
    assert_eq!(
        status,
        "same size and time · checking · 10.0 MB of 100.0 MB"
    );
}

#[test]
fn filters_keep_both_panes_aligned() {
    let (mut browser, calls) = started();
    deliver(&mut browser, &calls, root_rows());
    let cases = [
        (CompareFilter::All, vec!["docs", "a.txt", "b.txt", "old"]),
        (CompareFilter::Differences, vec!["b.txt", "old"]),
        (CompareFilter::Orphans, vec!["b.txt", "old"]),
        (CompareFilter::Same, vec!["docs", "a.txt"]),
    ];
    for (filter, expected) in cases {
        browser.compare_filter(filter);
        assert_eq!(paths(&browser, 0), expected, "{filter:?}");
        assert_eq!(paths(&browser, 1), expected, "{filter:?}");
    }
}

#[test]
fn the_bar_counts_differences_orphans_and_same_and_names_the_running_check() {
    let (mut browser, calls) = started();
    let totals = CompareTotals {
        rows: 9,
        same: 3,
        same_size_time: 1,
        different: 2,
        conflicts: 1,
        only_left: 1,
        only_right: 1,
        checking: 2,
        ..CompareTotals::default()
    };
    deliver(
        &mut browser,
        &calls,
        CompareUpdate::Totals {
            generation: 1,
            totals,
        },
    );
    let bar = bar_view(browser.compare_mode().expect("compare mode"));
    let line: Vec<(Tone, &str)> = bar
        .totals
        .iter()
        .map(|t| (t.tone, t.text.as_str()))
        .collect();
    assert_eq!(
        line,
        [
            (Tone::Change, "3 different"),
            (Tone::Delete, "1 only left"),
            (Tone::Add, "1 only right"),
            (Tone::Muted, "4 same"),
            (Tone::Busy, "2 checking"),
        ]
    );
    assert_eq!(bar.running.as_deref(), Some("Comparing…"));
    deliver(
        &mut browser,
        &calls,
        CompareUpdate::Finished { generation: 1 },
    );
    let bar = bar_view(browser.compare_mode().expect("compare mode"));
    assert_eq!(bar.running, None);
}

#[test]
fn the_bar_leaves_out_the_checking_count_when_nothing_waits() {
    let (browser, _calls) = started();
    let bar = bar_view(browser.compare_mode().expect("compare mode"));
    assert_eq!(bar.totals.len(), 4);
    assert_eq!(bar.filter, CompareFilter::All);
}

#[test]
fn empty_lists_say_why() {
    let (mut browser, calls) = started();
    let text = |browser: &crate::browser::Browser| {
        empty_text(browser.compare_mode().expect("compare mode"))
    };
    assert_eq!(text(&browser).as_deref(), Some("Comparing…"));
    let none = CompareTotals::default();
    deliver(
        &mut browser,
        &calls,
        CompareUpdate::Totals {
            generation: 1,
            totals: none,
        },
    );
    deliver(
        &mut browser,
        &calls,
        CompareUpdate::Finished { generation: 1 },
    );
    assert_eq!(text(&browser).as_deref(), Some("Both folders are empty."));
    deliver(
        &mut browser,
        &calls,
        rows(1, vec![row(1, "a.txt", RowStatus::Same)]),
    );
    assert_eq!(text(&browser), None);
    browser.compare_filter(CompareFilter::Differences);
    assert_eq!(
        text(&browser).as_deref(),
        Some("The folders are identical.")
    );
}

#[test]
fn a_filter_that_hides_everything_says_so_while_differences_exist() {
    let (mut browser, calls) = started();
    deliver(
        &mut browser,
        &calls,
        rows(1, vec![row(1, "a.txt", RowStatus::Different)]),
    );
    let totals = CompareTotals {
        rows: 1,
        different: 1,
        ..CompareTotals::default()
    };
    deliver(
        &mut browser,
        &calls,
        CompareUpdate::Totals {
            generation: 1,
            totals,
        },
    );
    deliver(
        &mut browser,
        &calls,
        CompareUpdate::Finished { generation: 1 },
    );
    browser.compare_filter(CompareFilter::Same);
    let mode = browser.compare_mode().expect("compare mode");
    assert_eq!(
        empty_text(mode).as_deref(),
        Some("No rows match this filter.")
    );
}

#[test]
fn a_filter_moves_a_hidden_selection_to_the_first_row_left() {
    let (mut browser, calls) = started();
    deliver(&mut browser, &calls, root_rows());
    select(&mut browser, "a.txt");
    browser.compare_filter(CompareFilter::Orphans);
    assert_eq!(selected(&browser).as_deref(), Some("b.txt"));
    browser.compare_filter(CompareFilter::All);
    assert_eq!(selected(&browser).as_deref(), Some("b.txt"));
}

#[test]
fn the_cursor_keys_walk_the_visible_rows_and_stop_at_the_ends() {
    let (mut browser, calls) = started();
    deliver(&mut browser, &calls, root_rows());
    assert!(browser.compare_cursor(CursorMove::Down));
    assert_eq!(selected(&browser).as_deref(), Some("docs"));
    browser.compare_cursor(CursorMove::Down);
    browser.compare_cursor(CursorMove::Down);
    assert_eq!(selected(&browser).as_deref(), Some("b.txt"));
    browser.compare_cursor(CursorMove::Last);
    assert_eq!(selected(&browser).as_deref(), Some("old"));
    browser.compare_cursor(CursorMove::Down);
    assert_eq!(selected(&browser).as_deref(), Some("old"));
    browser.compare_cursor(CursorMove::PageUp);
    assert_eq!(selected(&browser).as_deref(), Some("docs"));
    browser.compare_cursor(CursorMove::Up);
    assert_eq!(selected(&browser).as_deref(), Some("docs"));
    browser.compare_cursor(CursorMove::First);
    assert_eq!(selected(&browser).as_deref(), Some("docs"));
}

#[test]
fn the_revision_moves_with_rows_and_filter_but_not_with_the_selection() {
    let (mut browser, calls) = started();
    let revision = |browser: &crate::browser::Browser| {
        browser.compare_mode().expect("compare mode").revision()
    };
    let start = revision(&browser);
    deliver(&mut browser, &calls, root_rows());
    let after_rows = revision(&browser);
    assert!(after_rows > start);
    select(&mut browser, "a.txt");
    browser.compare_cursor(CursorMove::Down);
    assert_eq!(revision(&browser), after_rows);
    browser.compare_filter(CompareFilter::Same);
    assert!(revision(&browser) > after_rows);
}
