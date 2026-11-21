//! View tests of H3: a Folders row opens its pair in the compare screen, two files open at
//! once, the screen's Closed and WalkFile events move the tab.

use navo_compare_cli::args::SideSpec;
use std::fs;
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use sidewise_ui::window_view::ScreenEvent;

use crate::compare_tab_script::scripted_shell;
use crate::compare_tab_tests::{TempDir, submit, wait_for_row};
use crate::screens::compare_tab::{CompareRequest, CompareTab, TabView};
use crate::shell::Shell;
use crate::start_tabs::StartChoice;

type Opened = (AnyWindowHandle, Entity<Shell>);

/// Two folders with differing `a.txt` and `b.txt` and an equal `c.txt`; submitted and walked.
fn folders(cx: &mut TestAppContext, tag: &str) -> (Opened, Entity<CompareTab>, [TempDir; 2]) {
    let left = TempDir::new(&format!("{tag}-l"));
    let right = TempDir::new(&format!("{tag}-r"));
    for (name, one, two) in [
        ("a.txt", "one", "two"),
        ("b.txt", "x", "y"),
        ("c.txt", "s", "s"),
    ] {
        fs::write(left.path().join(name), one).expect("write left");
        fs::write(right.path().join(name), two).expect("write right");
    }
    let opened = scripted_shell(cx);
    submit(cx, &opened, [left.path(), right.path()]);
    assert!(wait_for_row(cx, &opened, "a.txt"), "rows reach the view");
    let tab = settled(cx, &opened);
    (opened, tab, [left, right])
}

/// Waits (bounded) until all three rows are in the session, then returns the active tab.
fn settled(cx: &mut TestAppContext, (window, view): &Opened) -> Entity<CompareTab> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        cx.run_until_parked();
        let tab = view.read_with(cx, |shell, _| shell.active_compare().cloned());
        let tab = tab.expect("compare tab");
        let ready = tab.read_with(cx, |tab, _| tab.row_id("c.txt").is_some());
        if ready || Instant::now() > deadline {
            cx.update_window(*window, |_, window, cx| window.render_frame(cx))
                .expect("window");
            return tab;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// Sends `event` from the compare screen inside the window, as the screen does.
fn emit(
    cx: &mut TestAppContext,
    (window, _): &Opened,
    tab: &Entity<CompareTab>,
    event: ScreenEvent,
) {
    let screen = tab.read_with(cx, |tab, _| tab.screen().clone());
    cx.update_window(*window, |_, _, cx| {
        screen.update(cx, |_, cx| cx.emit(event))
    })
    .expect("window");
    cx.run_until_parked();
}

/// Sends the Folders view's `OpenPair(id)` inside the window.
fn open_row(cx: &mut TestAppContext, (window, _): &Opened, tab: &Entity<CompareTab>, id: u64) {
    use crate::screens::folders_view::FoldersEvent;
    let folders = tab.read_with(cx, |tab, _| tab.folders().clone());
    cx.update_window(*window, |_, _, cx| {
        folders.update(cx, |_, cx| cx.emit(FoldersEvent::OpenPair(id)));
    })
    .expect("window");
    cx.run_until_parked();
}

/// The tab's view and whether the screen holds an open file diff.
fn state(cx: &mut TestAppContext, tab: &Entity<CompareTab>) -> (TabView, bool, Option<u64>) {
    let screen = tab.read_with(cx, |tab, _| tab.screen().clone());
    let open = screen.read_with(cx, |screen, _| screen.is_file_open());
    tab.read_with(cx, |tab, _| (tab.view(), open, tab.shown_pair()))
}

#[gpui_kit::test]
fn open_pair_shows_the_file_diff(cx: &mut TestAppContext) {
    let (opened, tab, _dirs) = folders(cx, "open");
    assert_eq!(state(cx, &tab).0, TabView::Folders);
    let id = tab
        .read_with(cx, |tab, _| tab.row_id("a.txt"))
        .expect("row");
    open_row(cx, &opened, &tab, id);
    assert_eq!(state(cx, &tab), (TabView::File, true, Some(id)));
}

#[gpui_kit::test]
fn two_files_open_their_pair_at_once(cx: &mut TestAppContext) {
    let dir = TempDir::new("files");
    let (left, right) = (dir.path().join("l.txt"), dir.path().join("r.txt"));
    fs::write(&left, "one").expect("write");
    fs::write(&right, "two").expect("write");
    let opened = scripted_shell(cx);
    let request = CompareRequest {
        left: SideSpec::Path(left),
        right: SideSpec::Path(right),
        choice: StartChoice::default(),
    };
    let (window, view) = &opened;
    cx.update_window(*window, |_, window, cx| {
        view.update(cx, |shell, cx| shell.open_compare((0, request), window, cx))
    })
    .expect("window")
    .expect("open");
    cx.run_until_parked();
    let tab = view.read_with(cx, |shell, _| shell.active_compare().cloned());
    assert_eq!(
        state(cx, &tab.expect("tab")),
        (TabView::File, true, Some(0))
    );
}

#[gpui_kit::test]
fn closed_returns_to_folders_and_selects_the_reveal(cx: &mut TestAppContext) {
    let (opened, tab, _dirs) = folders(cx, "closed");
    let id = tab
        .read_with(cx, |tab, _| tab.row_id("a.txt"))
        .expect("row");
    open_row(cx, &opened, &tab, id);
    let reveal = Some("b.txt".to_owned());
    emit(cx, &opened, &tab, ScreenEvent::Closed { reveal });
    assert_eq!(state(cx, &tab).0, TabView::Folders);
    let folders = tab.read_with(cx, |tab, _| tab.folders().clone());
    let selected = folders.read_with(cx, |view, _| view.selected_path());
    assert_eq!(selected.as_deref(), Some("b.txt"));
}

#[gpui_kit::test]
fn walk_file_opens_the_next_difference_and_stops_at_the_end(cx: &mut TestAppContext) {
    let (opened, tab, _dirs) = folders(cx, "walk");
    let id = |cx: &mut TestAppContext, name: &str| {
        tab.read_with(cx, |tab, _| tab.row_id(name)).expect("row")
    };
    let (a, b) = (id(cx, "a.txt"), id(cx, "b.txt"));
    open_row(cx, &opened, &tab, a);
    emit(
        cx,
        &opened,
        &tab,
        ScreenEvent::WalkFile { from: a, step: 1 },
    );
    assert_eq!(state(cx, &tab), (TabView::File, true, Some(b)));
    emit(
        cx,
        &opened,
        &tab,
        ScreenEvent::WalkFile { from: b, step: 1 },
    );
    assert_eq!(state(cx, &tab), (TabView::File, true, Some(b)));
}

/// Opening a differing row in the Stream shows the Stream.
#[gpui_kit::test]
fn open_stream_from_folders_shows_the_stream(cx: &mut TestAppContext) {
    use crate::screens::folders_view::FoldersEvent;
    let (opened, tab, _dirs) = folders(cx, "stream-one");
    let id = tab
        .read_with(cx, |tab, _| tab.row_id("a.txt"))
        .expect("row");
    let folders = tab.read_with(cx, |tab, _| tab.folders().clone());
    cx.update_window(opened.0, |_, _, cx| {
        folders.update(cx, |_, cx| cx.emit(FoldersEvent::OpenStream(id)));
    })
    .expect("window");
    cx.run_until_parked();
    assert_eq!(state(cx, &tab).0, TabView::Stream);
}
