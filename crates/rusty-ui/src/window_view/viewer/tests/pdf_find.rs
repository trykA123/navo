use gpui_kit::AppContext as _;
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt;

use super::pdf::{PAGES, Rig, bounds, keys, page_text, rig, shown, text, text_now};
use super::{redraw, visible};
use crate::window_view::tests::{settle, viewer_open};

fn volt_hits() -> usize {
    (0..PAGES)
        .map(|page| page_text(page).to_lowercase().matches("volt").count())
        .sum()
}

fn type_now(cx: &mut TestAppContext, rig: &Rig, typed: &str) {
    cx.update_window(rig.0, |_, window, cx| window.input(typed, cx))
        .expect("window");
}

fn find(cx: &mut TestAppContext, rig: &Rig, typed: &str) {
    keys(cx, rig, &["ctrl-f"]);
    type_now(cx, rig, typed);
    settle(cx, &rig.1);
    redraw(cx, rig.0);
}

#[gpui_kit::test]
fn the_count_grows_while_pages_are_still_indexed(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    keys(cx, &rig, &["ctrl-f"]);
    type_now(cx, &rig, "volt");
    let count = text_now(cx, rig.0, "pdf-find-count");
    assert_eq!(count, "1 of 2 so far");
    assert_eq!(text_now(cx, rig.0, "pdf-indexing"), "Indexing page 3 of 40");
    settle(cx, &rig.1);
    let total = volt_hits();
    assert_eq!(text(cx, rig.0, "pdf-find-count"), format!("1 of {total}"));
    assert!(!visible(cx, rig.0, "pdf-indexing"));
}

#[gpui_kit::test]
fn enter_shift_enter_and_f3_step_through_the_hits(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    find(cx, &rig, "volt");
    let total = volt_hits();
    keys(cx, &rig, &["enter"]);
    assert_eq!(text(cx, rig.0, "pdf-find-count"), format!("2 of {total}"));
    keys(cx, &rig, &["enter", "enter"]);
    assert_eq!(text(cx, rig.0, "pdf-find-count"), format!("4 of {total}"));
    keys(cx, &rig, &["shift-enter"]);
    assert_eq!(text(cx, rig.0, "pdf-find-count"), format!("3 of {total}"));
    keys(cx, &rig, &["escape"]);
    assert!(viewer_open(cx, &rig.1));
    assert!(!visible(cx, rig.0, "pdf-find-count"));
}

#[gpui_kit::test]
fn f3_on_the_pages_steps_and_shows_the_hit_on_its_page(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    find(cx, &rig, "undervoltage");
    keys(cx, &rig, &["f3"]);
    assert_eq!(text(cx, rig.0, "pdf-find-count"), "2 of 4");
    assert_eq!(text(cx, rig.0, "pdf-status"), "PDF · page 14 of 40");
    assert!(visible(cx, rig.0, "pdf-hit-current"));
    keys(cx, &rig, &["shift-f3"]);
    assert_eq!(text(cx, rig.0, "pdf-status"), "PDF · page 4 of 40");
}

#[gpui_kit::test]
fn match_case_and_whole_word_narrow_the_hits(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    find(cx, &rig, "volt");
    let all = volt_hits();
    keys(cx, &rig, &["alt-w"]);
    settle(cx, &rig.1);
    let words = text(cx, rig.0, "pdf-find-count");
    assert!(!words.ends_with(&format!(" of {all}")), "{words}");
    keys(cx, &rig, &["alt-w", "alt-c"]);
    settle(cx, &rig.1);
    assert_eq!(text(cx, rig.0, "pdf-find-count"), format!("1 of {all}"));
    keys(cx, &rig, &["ctrl-f", "ctrl-a"]);
    type_now(cx, &rig, "Volt");
    settle(cx, &rig.1);
    assert_eq!(text(cx, rig.0, "pdf-find-count"), "No hits");
}

#[gpui_kit::test]
fn the_hit_rail_marks_every_hit(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    assert!(!visible(cx, rig.0, "pdf-rail"));
    find(cx, &rig, "volt");
    let rail = text(cx, rig.0, "pdf-rail");
    assert_eq!(rail, format!("Hit rail: {} hits", volt_hits()));
}

#[gpui_kit::test]
fn the_hit_list_groups_by_page_and_enter_jumps(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    find(cx, &rig, "undervoltage");
    keys(cx, &rig, &["ctrl-i"]);
    assert!(visible(cx, rig.0, "pdf-right-drawer"));
    assert_eq!(text(cx, rig.0, "pdf-hits-summary"), "4 hits on 4 pages");
    let row = text(cx, rig.0, ("pdf-hit-row", 2usize));
    assert!(row.starts_with("Page 24: "), "{row}");
    assert!(row.contains("undervoltage lockout"), "{row}");
    keys(cx, &rig, &["down", "down", "enter"]);
    assert_eq!(text(cx, rig.0, "pdf-find-count"), "3 of 4");
    assert_eq!(text(cx, rig.0, "pdf-status"), "PDF · page 24 of 40");
    keys(cx, &rig, &["escape"]);
    assert!(!visible(cx, rig.0, "pdf-right-drawer"));
    assert!(viewer_open(cx, &rig.1));
}

#[gpui_kit::test]
fn ctrl_shift_f_also_opens_the_hit_list(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    keys(cx, &rig, &["ctrl-shift-f"]);
    assert!(visible(cx, rig.0, "pdf-right-drawer"));
    keys(cx, &rig, &["ctrl-shift-f"]);
    assert!(!visible(cx, rig.0, "pdf-right-drawer"));
}

#[gpui_kit::test]
fn ctrl_b_toggles_the_left_drawer_and_ctrl_p_opens_it_on_pages(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    keys(cx, &rig, &["ctrl-b"]);
    assert!(visible(cx, rig.0, "pdf-left-drawer"));
    assert!(shown(cx, rig.0, ("pdf-page-row", 0usize)));
    keys(cx, &rig, &["ctrl-b"]);
    assert!(!visible(cx, rig.0, "pdf-left-drawer"));
    keys(cx, &rig, &["ctrl-p"]);
    assert!(shown(cx, rig.0, ("pdf-page-row", 0usize)));
    keys(cx, &rig, &["down", "down", "enter"]);
    assert_eq!(text(cx, rig.0, "pdf-status"), "PDF · page 3 of 40");
}

#[gpui_kit::test]
fn esc_closes_the_top_drawer_then_find_then_the_viewer(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    find(cx, &rig, "volt");
    keys(cx, &rig, &["ctrl-b", "ctrl-f"]);
    assert!(visible(cx, rig.0, "pdf-left-drawer"));
    keys(cx, &rig, &["escape"]);
    assert!(!visible(cx, rig.0, "pdf-left-drawer"));
    assert!(visible(cx, rig.0, "pdf-find-count"));
    keys(cx, &rig, &["escape"]);
    assert!(!visible(cx, rig.0, "pdf-find-count"));
    assert!(viewer_open(cx, &rig.1));
    keys(cx, &rig, &["escape"]);
    assert!(!viewer_open(cx, &rig.1));
}

#[gpui_kit::test]
fn closing_one_drawer_gives_focus_to_the_other(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    keys(cx, &rig, &["ctrl-b", "ctrl-i", "escape"]);
    assert!(!visible(cx, rig.0, "pdf-right-drawer"));
    assert!(visible(cx, rig.0, "pdf-left-drawer"));
    keys(cx, &rig, &["down", "down", "enter"]);
    assert_eq!(text(cx, rig.0, "pdf-status"), "PDF · page 3 of 40");
}

#[gpui_kit::test]
fn drawers_never_cover_the_status_line(cx: &mut TestAppContext) {
    let rig = rig(cx, "a-book.pdf");
    keys(cx, &rig, &["ctrl-b", "ctrl-i"]);
    let status = bounds(cx, rig.0, "pdf-status");
    let stage = bounds(cx, rig.0, "pdf-stage");
    for drawer in ["pdf-left-drawer", "pdf-right-drawer"] {
        let shown = bounds(cx, rig.0, drawer);
        assert!(
            shown.bottom() <= status.top(),
            "{drawer} {shown:?} {status:?}"
        );
        assert!(
            shown.bottom() <= stage.bottom(),
            "{drawer} {shown:?} {stage:?}"
        );
    }
    assert_eq!(stage.size.width, bounds(cx, rig.0, "pdf").size.width);
}
