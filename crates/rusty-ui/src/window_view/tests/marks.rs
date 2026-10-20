use std::sync::Arc;

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use rusty_core::AppState;
use rusty_fs::FakeFs;

use super::{open_options, options_for, press};
use crate::window_view::FerriteWindow;

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
}

fn rig(cx: &mut TestAppContext) -> Rig {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    for name in ["a.txt", "b.txt", "c.md", "d.txt"] {
        fs.add_file(&format!("home/{name}"), 1);
    }
    let fs = Arc::new(fs);
    let state = AppState::new(fs.location("home"));
    let (window, view) = open_options(cx, options_for(fs, state));
    Rig { window, view }
}

impl Rig {
    fn press(&self, cx: &mut TestAppContext, keys: &[&str]) {
        for key in keys {
            press(cx, self.window, &self.view, key);
        }
    }

    fn marked(&self, cx: &mut TestAppContext) -> Vec<u64> {
        self.view.read_with(cx, |view, _| {
            view.browser.state().active_tab().selection().marked()
        })
    }

    fn footer(&self, cx: &mut TestAppContext) -> String {
        let pane = self
            .view
            .read_with(cx, |view, _| view.browser.state().focused_pane());
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .find(("status", pane.get()))
                .label()
                .map(str::to_owned)
        })
        .expect("window")
        .expect("label")
    }

    fn notice(&self, cx: &mut TestAppContext) -> Option<(String, bool)> {
        self.view.read_with(cx, |view, _| {
            view.status_notice()
                .map(|notice| (notice.text.to_string(), notice.is_error))
        })
    }
}

#[gpui_kit::test]
fn shift_down_three_times_marks_four_rows_and_shift_up_shrinks_the_span(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, &["shift-down", "shift-down", "shift-down"]);
    assert_eq!(rig.marked(cx), [0, 1, 2, 3]);
    rig.press(cx, &["shift-up"]);
    assert_eq!(rig.marked(cx), [0, 1, 2]);
}

#[gpui_kit::test]
fn shift_end_and_shift_home_extend_to_the_ends(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, &["shift-end"]);
    assert_eq!(rig.marked(cx), [0, 1, 2, 3, 4]);
    rig.press(cx, &["shift-home"]);
    assert_eq!(rig.marked(cx), [0]);
}

#[gpui_kit::test]
fn ctrl_a_and_ctrl_plus_mark_the_folders_too(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, &["ctrl-a"]);
    assert_eq!(rig.marked(cx), [0, 1, 2, 3, 4]);
    rig.press(cx, &["ctrl-shift-a", "ctrl-+"]);
    assert_eq!(rig.marked(cx), [0, 1, 2, 3, 4]);
}

#[gpui_kit::test]
fn ctrl_shift_a_clears_and_slash_brings_the_marks_back(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, &["down", "space", "space", "down", "space"]);
    assert_eq!(rig.marked(cx), [2]);
    rig.press(cx, &["ctrl-shift-a"]);
    assert_eq!(rig.marked(cx), Vec::<u64>::new());
    rig.press(cx, &["/"]);
    assert_eq!(rig.marked(cx), [2]);
    rig.press(cx, &["ctrl--"]);
    assert_eq!(rig.marked(cx), Vec::<u64>::new());
}

#[gpui_kit::test]
fn alt_plus_marks_the_txt_files_and_alt_minus_unmarks_them(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, &["down", "alt-+"]);
    assert_eq!(rig.marked(cx), [1, 2, 4]);
    rig.press(cx, &["alt--"]);
    assert_eq!(rig.marked(cx), Vec::<u64>::new());
}

#[gpui_kit::test]
fn alt_plus_on_a_folder_marks_nothing(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, &["alt-+"]);
    assert_eq!(rig.marked(cx), Vec::<u64>::new());
}

#[gpui_kit::test]
fn slash_with_nothing_saved_says_so_on_the_status_line(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, &["/"]);
    assert_eq!(
        rig.notice(cx),
        Some(("No saved marks match this folder".to_owned(), true))
    );
    assert_eq!(rig.marked(cx), Vec::<u64>::new());
    rig.press(cx, &["down"]);
    assert_eq!(rig.notice(cx), None);
}

#[gpui_kit::test]
fn typed_letters_still_filter(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, &["c"]);
    let filter = rig.view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab().id();
        view.browser.filter_text(tab).map(str::to_owned)
    });
    assert_eq!(filter.as_deref(), Some("c"));
}

#[gpui_kit::test]
fn the_footer_counts_marks_the_filter_hides(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, &["down", "shift-down"]);
    assert_eq!(rig.marked(cx), [1, 2]);
    assert!(!rig.footer(cx).contains("hidden by filter"));
    rig.press(cx, &["c"]);
    let footer = rig.footer(cx);
    assert!(footer.contains("2 hidden by filter"), "{footer}");
}
