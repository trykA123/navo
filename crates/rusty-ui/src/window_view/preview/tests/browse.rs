use gpui_kit::AppContext as _;
use std::sync::Arc;

use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt;
use rusty_core::ProviderPath;
use rusty_fs::{Attributes, EntryKind, FakeFs, FsEntry};

use super::{Rig, open_fake};
use crate::window_view::FerriteWindow;
use crate::window_view::preview::contents::RowKind;
use crate::window_view::preview::spot::Spot;

fn project() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_file("home/proj/src/main.rs", 300);
    fs.add_file("home/proj/src/lib.rs", 200);
    fs.add_file("home/proj/target/app.exe", 5_000);
    fs.add_file("home/proj/readme.md", 40);
    fs.add_file("home/zz.txt", 1);
    fs
}

fn is_done(view: &FerriteWindow) -> bool {
    view.preview
        .folder()
        .is_some_and(|folder| folder.contents.is_read() && !folder.scan.is_counting())
}

fn browsing(cx: &mut TestAppContext) -> (Rig, Arc<FakeFs>) {
    let fs = Arc::new(project());
    let rig = open_fake(cx, &fs);
    rig.open(cx);
    assert!(rig.wait_until(cx, is_done));
    rig.key(cx, "ctrl-shift-q");
    (rig, fs)
}

fn spot(cx: &mut TestAppContext, rig: &Rig) -> Option<String> {
    rig.read(cx, |view| {
        let folder = view.preview.folder()?;
        match folder.spot? {
            Spot::Largest(rank) => Some(format!("largest {rank}")),
            Spot::Tree(_) => match folder.contents.row(folder.spot_row()?)?.0 {
                RowKind::Entry(entry) => Some(entry.name.to_text()),
                RowKind::More(_) => None,
            },
        }
    })
}

fn seen(name: &str) -> Option<String> {
    Some(name.to_owned())
}

fn tree_len(cx: &mut TestAppContext, rig: &Rig) -> usize {
    rig.read(cx, |view| {
        view.preview
            .folder()
            .map_or(0, |folder| folder.contents.len())
    })
}

fn sheet_has_focus(cx: &mut TestAppContext, rig: &Rig) -> bool {
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        view.read(cx).preview.is_browsing(window, cx)
    })
    .expect("window")
}

fn is_ringed(cx: &mut TestAppContext, rig: &Rig) -> Option<bool> {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.find("preview-sheet").selected()
    })
    .expect("window")
}

fn list_cursor_bar(cx: &mut TestAppContext, rig: &Rig) -> bool {
    let pane = rig.active_pane(cx);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within(("list", pane))
            .try_find("cursor-bar")
            .is_some()
    })
    .expect("window")
}

fn hints(cx: &mut TestAppContext, rig: &Rig) -> Vec<String> {
    (0..9usize)
        .filter_map(|index| rig.label(cx, ("side-sheet-hint", index)))
        .collect()
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

fn cursor_name(view: &FerriteWindow) -> Option<String> {
    let tab = view.browser.state().active_tab();
    let row = tab.view().cursor;
    tab.listing()?.get(row).map(|entry| entry.name.clone())
}

#[gpui_kit::test]
fn ctrl_shift_q_focuses_the_sheet_with_a_chip_a_ring_and_a_passive_list_cursor(
    cx: &mut TestAppContext,
) {
    let fs = Arc::new(project());
    let rig = open_fake(cx, &fs);
    rig.open(cx);
    assert!(rig.wait_until(cx, is_done));
    assert!(list_cursor_bar(cx, &rig));
    assert_eq!(is_ringed(cx, &rig), Some(false));
    assert!(!rig.shows(cx, "preview-browsing"));
    rig.key(cx, "ctrl-shift-q");
    assert!(sheet_has_focus(cx, &rig));
    assert!(!rig.list_has_focus(cx));
    assert_eq!(
        rig.label(cx, "preview-browsing").as_deref(),
        Some("Browsing")
    );
    assert_eq!(is_ringed(cx, &rig), Some(true));
    assert!(!list_cursor_bar(cx, &rig));
    assert_eq!(spot(cx, &rig), seen("src"));
    assert_eq!(
        hints(cx, &rig),
        ["Open", "Go to", "Mark", "Recount", "Close"]
    );
}

#[gpui_kit::test]
fn arrows_cross_from_the_tree_rows_to_the_largest_rows(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    assert_eq!(tree_len(cx, &rig), 3);
    for _ in 0..3 {
        rig.key(cx, "down");
    }
    assert_eq!(spot(cx, &rig), seen("largest 0"));
    for _ in 0..10 {
        rig.key(cx, "down");
    }
    assert_eq!(spot(cx, &rig), seen("largest 3"));
    for _ in 0..4 {
        rig.key(cx, "up");
    }
    assert_eq!(spot(cx, &rig), seen("readme.md"));
    assert!(sheet_has_focus(cx, &rig));
}

#[gpui_kit::test]
fn left_and_right_fold_the_tree(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    rig.key(cx, "right");
    assert!(rig.wait_until(cx, |view| {
        view.preview.folder().is_some_and(|f| f.contents.len() == 5)
    }));
    rig.key(cx, "right");
    assert_eq!(spot(cx, &rig), seen("lib.rs"));
    rig.key(cx, "left");
    assert_eq!(spot(cx, &rig), seen("src"));
    rig.key(cx, "left");
    assert_eq!(tree_len(cx, &rig), 3);
    assert_eq!(spot(cx, &rig), seen("src"));
}

#[gpui_kit::test]
fn shift_enter_on_a_folder_navigates_the_active_pane_into_it(cx: &mut TestAppContext) {
    let (rig, fs) = browsing(cx);
    rig.key(cx, "shift-enter");
    assert_eq!(location(cx, &rig), fs.location("home/proj/src").as_str());
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn shift_enter_on_a_largest_file_reveals_it_with_the_cursor_on_it(cx: &mut TestAppContext) {
    let (rig, fs) = browsing(cx);
    for _ in 0..3 {
        rig.key(cx, "down");
    }
    rig.key(cx, "shift-enter");
    let folder = fs.location("home/proj/target");
    assert!(rig.wait_until(cx, |view| {
        view.browser.state().active_tab().location() == &folder
            && cursor_name(view).as_deref() == Some("app.exe")
    }));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn f3_on_a_file_opens_the_full_viewer(cx: &mut TestAppContext) {
    let (rig, fs) = browsing(cx);
    for _ in 0..3 {
        rig.key(cx, "down");
    }
    rig.key(cx, "f3");
    let file = fs.location("home/proj/target/app.exe");
    assert_eq!(
        rig.read(cx, |view| view.viewer_location().cloned()),
        Some(file)
    );
}

#[gpui_kit::test]
fn a_click_selects_and_browses_and_a_double_click_opens_in_the_preview(cx: &mut TestAppContext) {
    let fs = Arc::new(project());
    let rig = open_fake(cx, &fs);
    rig.open(cx);
    assert!(rig.wait_until(cx, is_done));
    rig.click(cx, ("preview-node", 2usize));
    assert!(sheet_has_focus(cx, &rig));
    assert_eq!(spot(cx, &rig), seen("readme.md"));
    assert_eq!(tree_len(cx, &rig), 3);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.double_click(("preview-node", 0usize), cx);
    })
    .expect("window");
    rig.redraw(cx);
    assert!(rig.wait_until(cx, |view| {
        is_done(view)
            && view
                .preview
                .folder()
                .is_some_and(|f| f.target.name() == "src")
    }));
    assert_eq!(location(cx, &rig), fs.location("home").as_str());
    assert!(sheet_has_focus(cx, &rig));
}

#[gpui_kit::test]
fn escape_closes_the_sheet_and_restores_the_list_cursor(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    rig.key(cx, "escape");
    assert!(rig.list_has_focus(cx));
    assert!(!sheet_has_focus(cx, &rig));
    assert!(list_cursor_bar(cx, &rig));
    assert!(!rig.read(cx, |view| view.preview.is_open()));
}

#[gpui_kit::test]
fn tab_leaves_browse_mode_then_switches_panes(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    let before = rig.active_pane(cx);
    rig.key(cx, "tab");
    assert_ne!(rig.active_pane(cx), before);
    assert!(rig.list_has_focus(cx));
    assert!(!sheet_has_focus(cx, &rig));
}

#[gpui_kit::test]
fn the_close_button_closes_the_sheet_and_leaves_browse_mode(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    rig.click(cx, "side-sheet-close");
    assert!(!rig.read(cx, |view| view.preview.is_open()));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn ctrl_shift_q_on_a_closed_sheet_selects_the_first_row_once_the_tree_arrives(
    cx: &mut TestAppContext,
) {
    let fs = Arc::new(project());
    let rig = open_fake(cx, &fs);
    rig.key(cx, "ctrl-shift-q");
    rig.settle_motion(cx);
    assert!(rig.wait_until(cx, is_done));
    assert!(sheet_has_focus(cx, &rig));
    assert_eq!(spot(cx, &rig), seen("src"));
}

#[gpui_kit::test]
fn the_spot_stays_on_its_node_when_a_fold_above_it_moves_the_rows(cx: &mut TestAppContext) {
    let (rig, _) = browsing(cx);
    rig.key(cx, "down");
    rig.key(cx, "down");
    assert_eq!(spot(cx, &rig), seen("readme.md"));
    rig.click(cx, ("preview-chevron", 0usize));
    assert!(rig.wait_until(cx, |view| {
        view.preview.folder().is_some_and(|f| f.contents.len() == 5)
    }));
    assert_eq!(spot(cx, &rig), seen("readme.md"));
    rig.key(cx, "up");
    assert_eq!(spot(cx, &rig), seen("target"));
}

#[gpui_kit::test]
fn a_folder_reload_keeps_focus_and_the_spot_on_the_same_node(cx: &mut TestAppContext) {
    let fs = Arc::new(project().with_watch());
    let rig = open_fake(cx, &fs);
    rig.open(cx);
    assert!(rig.wait_until(cx, is_done));
    rig.key(cx, "ctrl-shift-q");
    rig.key(cx, "down");
    assert_eq!(spot(cx, &rig), seen("target"));
    let loads = rig.loads(cx);
    fs.add_entry(
        "home",
        FsEntry {
            name: ProviderPath::from_text("proj"),
            kind: EntryKind::Folder,
            size: 0,
            modified_ms: Some(7_000),
            attributes: Attributes::default(),
        },
    );
    assert!(rig.rest_until(cx, |view| view.preview.loads() > loads && is_done(view)));
    assert!(sheet_has_focus(cx, &rig));
    assert_eq!(spot(cx, &rig), seen("target"));
}
