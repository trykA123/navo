//! The network stop and continue, folders that hold differences, and which panes can compare.

use std::sync::atomic::Ordering;

use super::mode::{deliver, select, selected, start, visible};
use super::{browser, row, rows, sides};
use crate::browser::{
    Browser, CompareFilter, CompareRefusal, CompareRow, CompareTotals, CompareUpdate, RowStatus,
    StopBound, Tone, bar_view, pane_rows,
};
use crate::settings::Values;

fn started() -> (Browser, std::sync::Arc<super::Calls>) {
    let (mut browser, calls) = browser(&sides());
    start(&mut browser);
    (browser, calls)
}

fn stopped(generation: u64) -> CompareUpdate {
    CompareUpdate::Stopped {
        generation,
        entries: 200_000,
        bound: StopBound::Entries,
    }
}

fn revision(browser: &Browser) -> u64 {
    browser.compare_mode().expect("compare mode").revision()
}

fn stopped_entries(browser: &Browser) -> Option<u64> {
    browser.compare_mode().expect("compare mode").stopped()
}

/// `a/b/x.txt` differs; `a` and `a/b` are same folders that hold it; `c` holds nothing.
fn nested_change() -> CompareUpdate {
    let holder = |id, path| CompareRow {
        contains_differences: true,
        ..row(id, path, RowStatus::Same)
    };
    rows(
        1,
        vec![
            holder(1, "a"),
            row(2, "c", RowStatus::Same),
            holder(3, "a/b"),
            row(4, "a/b/x.txt", RowStatus::Different),
        ],
    )
}

#[test]
fn a_walk_stopped_at_the_limit_shows_its_notice_and_keeps_the_rows() {
    let (mut browser, calls) = started();
    deliver(&mut browser, &calls, nested_change());
    deliver(&mut browser, &calls, stopped(1));
    assert_eq!(stopped_entries(&browser), Some(200_000));
    let mode = browser.compare_mode().expect("compare mode");
    let notice = bar_view(mode).stopped.expect("a stop notice");
    assert!(notice.contains("Continue?"), "{notice}");
    assert_eq!(visible(&browser), ["a", "c"]);
}

#[test]
fn continue_asks_the_engine_once_per_stop_and_the_notice_goes_when_it_continued() {
    let (mut browser, calls) = started();
    assert!(!browser.compare_continue());
    assert_eq!(calls.continued.load(Ordering::SeqCst), 0);
    deliver(&mut browser, &calls, stopped(1));
    assert!(browser.compare_continue());
    assert_eq!(calls.continued.load(Ordering::SeqCst), 1);
    assert_eq!(stopped_entries(&browser), Some(200_000));
    deliver(
        &mut browser,
        &calls,
        CompareUpdate::Continued { generation: 1 },
    );
    assert_eq!(stopped_entries(&browser), None);
    let mode = browser.compare_mode().expect("compare mode");
    assert_eq!(bar_view(mode).stopped, None);
    assert!(!browser.compare_continue());
    assert_eq!(calls.continued.load(Ordering::SeqCst), 1);
}

#[test]
fn a_stop_of_an_older_generation_is_ignored_and_a_newer_generation_clears_it() {
    let (mut browser, calls) = started();
    deliver(
        &mut browser,
        &calls,
        rows(2, vec![row(1, "a.txt", RowStatus::Same)]),
    );
    deliver(&mut browser, &calls, stopped(1));
    assert_eq!(stopped_entries(&browser), None);
    deliver(&mut browser, &calls, stopped(2));
    assert_eq!(stopped_entries(&browser), Some(200_000));
    deliver(
        &mut browser,
        &calls,
        rows(3, vec![row(1, "a.txt", RowStatus::Same)]),
    );
    assert_eq!(stopped_entries(&browser), None);
}

#[test]
fn a_continued_of_an_older_generation_keeps_the_stop() {
    let (mut browser, calls) = started();
    deliver(&mut browser, &calls, stopped(2));
    deliver(
        &mut browser,
        &calls,
        CompareUpdate::Continued { generation: 1 },
    );
    assert_eq!(stopped_entries(&browser), Some(200_000));
}

#[test]
fn only_rows_of_the_folder_shown_move_the_revision() {
    let (mut browser, calls) = started();
    deliver(&mut browser, &calls, nested_change());
    let before = revision(&browser);
    let totals = CompareTotals {
        rows: 4,
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
    deliver(&mut browser, &calls, stopped(1));
    deliver(
        &mut browser,
        &calls,
        CompareUpdate::Finished { generation: 1 },
    );
    deliver(
        &mut browser,
        &calls,
        rows(1, vec![row(5, "a/b/y.txt", RowStatus::Same)]),
    );
    assert_eq!(revision(&browser), before);
    deliver(
        &mut browser,
        &calls,
        rows(1, vec![row(6, "d", RowStatus::Same)]),
    );
    assert!(revision(&browser) > before);
}

#[test]
fn a_folder_that_holds_a_difference_stays_under_the_differences_filter() {
    let (mut browser, calls) = started();
    deliver(&mut browser, &calls, nested_change());
    browser.compare_filter(CompareFilter::Differences);
    assert_eq!(visible(&browser), ["a"]);
}

#[test]
fn folders_with_nested_differences_get_a_distinct_tone_and_status() {
    let (mut browser, calls) = started();
    deliver(&mut browser, &calls, nested_change());
    for side in [0, 1] {
        let mode = browser.compare_mode().expect("compare mode");
        let views = pane_rows(mode, side);
        let changed = views.iter().find(|row| row.path == "a").expect("folder");
        let same = views.iter().find(|row| row.path == "c").expect("folder");
        assert_eq!(changed.tone, Tone::Change);
        assert_ne!(
            changed.status,
            navo_l10n::tr("compare-status-different"),
            "a folder holding a difference is not drawn as one that differs"
        );
        assert_eq!(
            changed.status,
            navo_l10n::tr("compare-status-contains-differences")
        );
        assert_eq!(same.tone, Tone::Muted);
        assert!(same.status.is_empty());
    }
    select(&mut browser, "a");
    assert!(browser.compare_enter());
    let mode = browser.compare_mode().expect("compare mode");
    let holder = &pane_rows(mode, 0)[0];
    assert_eq!(holder.tone, Tone::Change);
    assert_eq!(
        holder.status,
        navo_l10n::tr("compare-status-contains-differences")
    );
}

#[test]
fn nested_differences_preserve_one_sided_and_checking_folder_tones() {
    let (mut browser, calls) = started();
    for (status, side) in [
        (RowStatus::OnlyLeft, 0),
        (RowStatus::OnlyRight, 1),
        (RowStatus::Checking, 0),
        (RowStatus::NotChecked("compare-reason-stopped"), 0),
        (RowStatus::Unavailable("compare-reason-read-error"), 0),
    ] {
        let folder = CompareRow {
            contains_differences: true,
            ..row(1, "a", status)
        };
        deliver(&mut browser, &calls, rows(1, vec![folder]));
        let mode = browser.compare_mode().expect("compare mode");
        let view = &pane_rows(mode, side)[0];
        let (tone, _) = super::super::view::look_of(status);
        assert_eq!(view.tone, tone, "{status:?} keeps its own colour");
        assert_eq!(view.status, super::super::view::status_text(status, None));
    }
}

#[test]
fn next_difference_from_the_roots_lands_on_the_folder_and_enter_goes_in() {
    let (mut browser, calls) = started();
    deliver(&mut browser, &calls, nested_change());
    assert!(browser.compare_next_difference());
    assert_eq!(selected(&browser).as_deref(), Some("a"));
    assert!(!browser.compare_next_difference());
    select(&mut browser, "c");
    assert!(browser.compare_previous_difference());
    assert_eq!(selected(&browser).as_deref(), Some("a"));
}

#[test]
fn a_pane_the_hooks_cannot_read_does_not_offer_compare() {
    let (mut browser, calls) = browser(&sides());
    assert!(browser.can_compare());
    calls.unsupported.store(true, Ordering::SeqCst);
    assert!(!browser.can_compare());
    let refused = browser.compare_with_other_pane(&Values::default());
    assert_eq!(refused, Err(CompareRefusal::NotAvailable));
    assert!(browser.compare_mode().is_none());
    calls.unsupported.store(false, Ordering::SeqCst);
    assert!(browser.can_compare());
}
