//! The empty-folder and filtered-out lines: shown for a finished listing only, never while loading.

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Role, TestAppContext};

use super::{Harness, open_list, props};
use crate::file_list::FileListProps;

/// Opens a list with no rows, marks it finished or loading, and renders one more frame.
fn open_empty(
    cx: &mut TestAppContext,
    (is_finished, filter, listed): (bool, Option<&str>, u64),
) -> gpui_kit::AnyWindowHandle {
    let mut empty: FileListProps = props(Vec::new().into());
    empty.filter = filter.map(Into::into);
    let (window, harness) = open_list(cx, empty);
    harness.update(cx, |harness: &mut Harness, cx| {
        harness.is_finished = is_finished;
        harness.listed = listed;
        cx.notify();
    });
    window
}

#[gpui_kit::test]
fn a_finished_empty_listing_renders_the_empty_folder_line(cx: &mut TestAppContext) {
    let window = open_empty(cx, (true, None, 0));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let line = window.within("left").find("list-empty");
        assert_eq!(line.role(), Some(Role::Status));
        assert_eq!(line.label(), Some("This folder is empty"));
    })
    .expect("window");
}

#[gpui_kit::test]
fn a_loading_pane_renders_no_empty_line(cx: &mut TestAppContext) {
    let window = open_empty(cx, (false, None, 0));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.within("left").try_find("list-empty").is_none());
    })
    .expect("window");
}

#[gpui_kit::test]
fn a_filtered_out_listing_renders_the_filter_text(cx: &mut TestAppContext) {
    let window = open_empty(cx, (true, Some("zz"), 3));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let line = window.within("left").find("list-empty");
        assert_eq!(
            line.label(),
            Some("No files match the filter. Esc clears it.")
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn a_filter_on_an_empty_folder_still_says_the_folder_is_empty(cx: &mut TestAppContext) {
    let window = open_empty(cx, (true, Some("zz"), 0));
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let line = window.within("left").find("list-empty");
        assert_eq!(line.label(), Some("This folder is empty"));
    })
    .expect("window");
}
