use gpui_kit::AppContext as _;
use std::sync::Arc;

use gpui_kit::TestAppContext;
use rusty_fs::FakeFs;

use super::{Rig, open_fake};
use crate::window_view::FerriteWindow;

fn project() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_file("home/proj/src/main.rs", 300);
    fs.add_file("home/proj/readme.md", 40);
    fs.add_file("home/zz.txt", 1);
    fs
}

fn is_done(view: &FerriteWindow) -> bool {
    view.preview
        .folder()
        .is_some_and(|folder| folder.contents.is_read() && !folder.scan.is_counting())
}

fn is_open(cx: &mut TestAppContext, rig: &Rig) -> bool {
    rig.read(cx, |view| view.preview.is_open())
}

fn sheet_has_focus(cx: &mut TestAppContext, rig: &Rig) -> bool {
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.read(cx).preview.is_browsing(window, cx)
    })
    .expect("window")
}

fn filter(cx: &mut TestAppContext, rig: &Rig) -> Option<String> {
    rig.read(cx, |view| {
        let tab = view.browser.state().active_tab().id();
        view.browser.filter_text(tab).map(str::to_owned)
    })
}

fn location(cx: &mut TestAppContext, rig: &Rig) -> String {
    rig.read(cx, |view| {
        view.browser
            .state()
            .active_tab()
            .location()
            .as_str()
            .to_owned()
    })
}

fn rig_on(cx: &mut TestAppContext, fs: FakeFs) -> (Rig, Arc<FakeFs>) {
    let fs = Arc::new(fs);
    let rig = open_fake(cx, &fs);
    (rig, fs)
}

#[gpui_kit::test]
fn f3_on_a_folder_opens_the_preview_on_it_and_the_list_keeps_focus(cx: &mut TestAppContext) {
    let (rig, _) = rig_on(cx, project());
    rig.key(cx, "f3");
    assert!(is_open(cx, &rig));
    rig.settle_motion(cx);
    assert!(rig.wait_until(cx, is_done));
    assert_eq!(rig.shown_name(cx).as_deref(), Some("proj"));
    assert!(rig.read(cx, |view| view.viewer.is_none()));
    assert!(rig.list_has_focus(cx));
    assert!(!sheet_has_focus(cx, &rig));
}

#[gpui_kit::test]
fn f3_on_the_folder_the_sheet_shows_moves_focus_into_it(cx: &mut TestAppContext) {
    let (rig, _) = rig_on(cx, project());
    rig.open(cx);
    assert!(rig.wait_until(cx, is_done));
    rig.key(cx, "f3");
    assert!(is_open(cx, &rig));
    assert!(sheet_has_focus(cx, &rig));
    assert!(!rig.list_has_focus(cx));
    assert_eq!(
        rig.label(cx, "preview-browsing").as_deref(),
        Some("Browsing")
    );
    assert_eq!(rig.shown_name(cx).as_deref(), Some("proj"));
    assert!(rig.read(cx, |view| view.viewer.is_none()));
}

#[gpui_kit::test]
fn f3_on_a_file_still_opens_the_viewer_and_leaves_the_sheet_closed(cx: &mut TestAppContext) {
    let (rig, fs) = rig_on(cx, project());
    rig.key(cx, "down");
    rig.key(cx, "f3");
    assert_eq!(
        rig.read(cx, |view| view.viewer_location().cloned()),
        Some(fs.location("home/zz.txt"))
    );
    assert!(!is_open(cx, &rig));
}

#[gpui_kit::test]
fn f3_without_a_row_under_the_cursor_does_nothing(cx: &mut TestAppContext) {
    let fs = FakeFs::new();
    fs.add_folder("home");
    let (rig, _) = rig_on(cx, fs);
    rig.key(cx, "f3");
    assert!(!is_open(cx, &rig));
    assert!(rig.read(cx, |view| view.viewer.is_none()));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn escape_in_browse_mode_closes_the_sheet_and_returns_focus_to_the_list(cx: &mut TestAppContext) {
    let (rig, _) = rig_on(cx, project());
    rig.open(cx);
    assert!(rig.wait_until(cx, is_done));
    rig.key(cx, "ctrl-shift-q");
    assert!(sheet_has_focus(cx, &rig));
    rig.key(cx, "escape");
    assert!(!is_open(cx, &rig));
    assert!(rig.list_has_focus(cx));
    rig.settle_motion(cx);
    assert!(!rig.shows(cx, "preview-sheet"));
}

#[gpui_kit::test]
fn escape_in_the_list_clears_the_filter_then_closes_the_sheet(cx: &mut TestAppContext) {
    let (rig, _) = rig_on(cx, project());
    rig.open(cx);
    assert!(rig.wait_until(cx, is_done));
    rig.key(cx, "p");
    assert_eq!(filter(cx, &rig).as_deref(), Some("p"));
    rig.key(cx, "escape");
    assert_eq!(filter(cx, &rig), None);
    assert!(is_open(cx, &rig));
    rig.key(cx, "escape");
    assert!(!is_open(cx, &rig));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn escape_with_the_sheet_closed_only_clears_the_filter(cx: &mut TestAppContext) {
    let (rig, fs) = rig_on(cx, project());
    rig.key(cx, "p");
    assert_eq!(filter(cx, &rig).as_deref(), Some("p"));
    rig.key(cx, "escape");
    assert_eq!(filter(cx, &rig), None);
    rig.key(cx, "escape");
    assert!(!is_open(cx, &rig));
    assert!(rig.read(cx, |view| view.viewer.is_none()));
    assert_eq!(location(cx, &rig), fs.location("home").as_str());
    assert!(rig.list_has_focus(cx));
}
