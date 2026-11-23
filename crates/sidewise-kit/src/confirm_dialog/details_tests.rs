//! The summary's second line: text after a line break in the summary wraps on its own line
//! below the sizes, so it is never cut with an ellipsis.

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
