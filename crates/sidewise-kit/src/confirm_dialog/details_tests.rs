//! The summary's second line wraps whole below the sizes, and the details list scrolls by row
//! and by page without passing either end.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::tests::{DIALOG, Harness, open_harness, props};

#[gpui_kit::test]
fn a_second_summary_line_shows_whole_below_the_first(cx: &mut TestAppContext) {
    let mut two = props(1, 0);
    two.summary = "7 files, 1 folder · 12,345 B\n3 items already there will be skipped".into();
    let (window, _) = open_harness(cx, Harness::new(two));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let dialog = window.within(DIALOG);
        let first = dialog.find("summary");
        assert_eq!(first.label(), Some("7 files, 1 folder · 12,345 B"));
        let second = dialog.find("summary-detail");
        assert_eq!(
            second.label(),
            Some("3 items already there will be skipped")
        );
    })
    .expect("window");
}

#[test]
fn the_list_scrolls_by_row_and_page_and_stops_at_both_ends() {
    use super::details::scrolled;
    assert_eq!(scrolled("down", 0., 500.), Some(-44.));
    assert_eq!(scrolled("up", -20., 500.), Some(0.));
    assert_eq!(scrolled("pagedown", -400., 500.), Some(-500.));
    assert_eq!(scrolled("pageup", -500., 500.), Some(-264.));
    assert_eq!(scrolled("end", 0., 500.), Some(-500.));
    assert_eq!(scrolled("home", -300., 500.), Some(0.));
    assert_eq!(scrolled("a", 0., 500.), None);
}
