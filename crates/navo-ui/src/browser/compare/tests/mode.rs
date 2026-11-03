use std::sync::atomic::Ordering;

use super::super::super::tests::settle;
use super::super::{CompareFilter, CompareRefusal, CompareUpdate, RowStatus};
use super::{browser, cursor_names, cursor_on, pane_locations, row, rows, sides};
use crate::browser::Browser;
use crate::settings::Values;

pub(super) fn start(browser: &mut Browser) {
    browser
        .compare_with_other_pane(&Values::default())
        .expect("compare starts");
}

pub(super) fn deliver(browser: &mut Browser, calls: &super::Calls, update: CompareUpdate) {
    calls.send(update);
    browser.poll();
}

pub(super) fn selected(browser: &Browser) -> Option<String> {
    let mode = browser.compare_mode()?;
    mode.selected().map(|row| row.path.clone())
}

pub(super) fn visible(browser: &Browser) -> Vec<String> {
    let mode = browser.compare_mode().expect("compare mode");
    mode.visible().iter().map(|row| row.path.clone()).collect()
}

pub(super) fn select(browser: &mut Browser, path: &str) {
    browser
        .compare_mode_mut()
        .expect("compare mode")
        .select(path);
}

/// Root rows: docs (same), a.txt (same), b.txt (only left), old (only left).
pub(super) fn root_rows() -> CompareUpdate {
    rows(
        1,
        vec![
            row(1, "docs", RowStatus::Same),
            row(2, "a.txt", RowStatus::Same),
            row(3, "b.txt", RowStatus::OnlyLeft),
            row(4, "old", RowStatus::OnlyLeft),
            row(5, "docs/report.txt", RowStatus::Different),
        ],
    )
}

#[test]
fn starting_compares_the_left_pane_with_the_right_pane() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    let starts = calls.starts.lock().unwrap().clone();
    let expected = (fs.location("left"), fs.location("right"));
    let expected = (
        expected.0.as_str().to_owned(),
        expected.1.as_str().to_owned(),
    );
    assert_eq!(starts, [expected]);
}

#[test]
fn a_single_pane_cannot_compare() {
    let fs = sides();
    let (mut browser, _calls) = browser(&fs);
    browser
        .command(&navo_core::Command::LayoutClose)
        .expect("close");
    let refused = browser.compare_with_other_pane(&Values::default());
    assert_eq!(refused, Err(CompareRefusal::NoOtherPane));
    assert!(browser.compare_mode().is_none());
}

#[test]
fn leaving_restores_both_panes_and_their_cursors() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    cursor_on(&mut browser, 1);
    let before = (pane_locations(&browser), cursor_names(&browser));
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    select(&mut browser, "docs");
    assert!(browser.compare_enter());
    settle(&mut browser);
    assert_ne!(pane_locations(&browser), before.0);
    browser.leave_compare();
    settle(&mut browser);
    assert_eq!((pane_locations(&browser), cursor_names(&browser)), before);
    assert!(browser.compare_mode().is_none());
}

#[test]
fn leaving_drops_the_running_compare() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    assert!(!calls.dropped.load(Ordering::SeqCst));
    browser.leave_compare();
    assert!(calls.dropped.load(Ordering::SeqCst));
}

#[test]
fn enter_on_a_folder_pair_opens_it_in_both_panes() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    select(&mut browser, "docs");
    assert!(browser.compare_enter());
    settle(&mut browser);
    let docs = ["left/docs", "right/docs"].map(|p| fs.location(p).as_str().to_owned());
    assert_eq!(pane_locations(&browser), docs);
    assert_eq!(visible(&browser), ["docs/report.txt"]);
}

#[test]
fn enter_on_a_one_sided_folder_does_nothing() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    let before = pane_locations(&browser);
    select(&mut browser, "old");
    assert!(!browser.compare_enter());
    assert_eq!(pane_locations(&browser), before);
}

#[test]
fn backspace_goes_up_in_both_panes_and_selects_the_folder_left() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    let roots = pane_locations(&browser);
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    select(&mut browser, "docs");
    browser.compare_enter();
    settle(&mut browser);
    assert!(browser.compare_up());
    settle(&mut browser);
    assert_eq!(pane_locations(&browser), roots);
    assert_eq!(
        cursor_names(&browser),
        [Some("docs".into()), Some("docs".into())]
    );
    assert_eq!(selected(&browser).as_deref(), Some("docs"));
}

#[test]
fn backspace_at_the_roots_does_nothing() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    let roots = pane_locations(&browser);
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    assert!(!browser.compare_up());
    settle(&mut browser);
    assert_eq!(pane_locations(&browser), roots);
}

#[test]
fn enter_on_a_file_row_asks_for_the_file_diff() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    select(&mut browser, "b.txt");
    assert!(browser.compare_enter());
    let asked = browser.take_file_diff().expect("a file diff request");
    assert_eq!(asked.path, "b.txt");
    assert_eq!(asked.left, Some(fs.location("left/b.txt")));
    assert_eq!(asked.right, None);
}

#[test]
fn next_difference_skips_same_rows_and_stops_at_the_last() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    assert!(browser.compare_next_difference());
    assert_eq!(selected(&browser).as_deref(), Some("b.txt"));
    assert!(browser.compare_next_difference());
    assert_eq!(selected(&browser).as_deref(), Some("old"));
    assert!(!browser.compare_next_difference());
    assert_eq!(selected(&browser).as_deref(), Some("old"));
}

#[test]
fn previous_difference_skips_same_rows_and_stops_at_the_first() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    select(&mut browser, "a.txt");
    assert!(!browser.compare_previous_difference());
    assert_eq!(selected(&browser).as_deref(), Some("a.txt"));
    select(&mut browser, "old");
    assert!(browser.compare_previous_difference());
    assert_eq!(selected(&browser).as_deref(), Some("b.txt"));
    assert!(!browser.compare_previous_difference());
}

#[test]
fn esc_clears_the_filter_before_leaving() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    let mode = browser.compare_mode_mut().expect("compare mode");
    mode.set_filter(CompareFilter::Orphans);
    assert_eq!(visible(&browser), ["b.txt", "old"]);
    assert!(browser.compare_exit());
    let filter = browser.compare_mode().map(|mode| mode.filter());
    assert_eq!(filter, Some(CompareFilter::All));
    assert!(browser.compare_exit());
    assert!(browser.compare_mode().is_none());
    assert!(!browser.compare_exit());
}

#[test]
fn a_newer_generation_drops_the_rows_received_before() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    deliver(
        &mut browser,
        &calls,
        rows(2, vec![row(9, "a.txt", RowStatus::Different)]),
    );
    assert_eq!(visible(&browser), ["a.txt"]);
    let mode = browser.compare_mode().expect("compare mode");
    assert_eq!((mode.generation(), mode.received()), (2, 1));
    assert!(mode.checking());
}

#[test]
fn a_stale_generation_is_ignored() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    deliver(
        &mut browser,
        &calls,
        rows(2, vec![row(9, "a.txt", RowStatus::Same)]),
    );
    deliver(&mut browser, &calls, root_rows());
    deliver(
        &mut browser,
        &calls,
        CompareUpdate::Finished { generation: 1 },
    );
    assert_eq!(visible(&browser), ["a.txt"]);
    assert!(browser.compare_mode().is_some_and(|mode| mode.checking()));
}

#[test]
fn finished_ends_checking_for_its_generation() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    deliver(&mut browser, &calls, root_rows());
    deliver(
        &mut browser,
        &calls,
        CompareUpdate::Finished { generation: 1 },
    );
    assert!(browser.compare_mode().is_some_and(|mode| !mode.checking()));
}

#[test]
fn an_update_of_a_left_compare_is_ignored() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    let old_sink = calls.sink.lock().unwrap().clone().expect("sink");
    browser.leave_compare();
    start(&mut browser);
    old_sink(root_rows());
    browser.poll();
    assert_eq!(browser.compare_mode().map(|mode| mode.received()), Some(0));
}

#[test]
fn a_failed_start_leaves_compare_mode_with_a_notice() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    deliver(&mut browser, &calls, CompareUpdate::Failed("no".into()));
    assert!(browser.compare_mode().is_none());
    assert_eq!(browser.take_compare_notice().as_deref(), Some("no"));
    assert!(calls.dropped.load(Ordering::SeqCst));
}

#[test]
fn stop_checking_stops_the_checked_row_or_else_all_rows() {
    let fs = sides();
    let (mut browser, calls) = browser(&fs);
    start(&mut browser);
    deliver(
        &mut browser,
        &calls,
        rows(1, vec![row(7, "a.txt", RowStatus::Checking)]),
    );
    select(&mut browser, "a.txt");
    assert!(browser.compare_stop_checking());
    deliver(
        &mut browser,
        &calls,
        rows(1, vec![row(7, "a.txt", RowStatus::Same)]),
    );
    assert!(browser.compare_stop_checking());
    assert_eq!(calls.stopped(), [Some(7), None]);
}
