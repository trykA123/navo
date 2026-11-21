//! View tests of Recent: a compare is recorded once its run opened both sides, never when a
//! side failed to open (TN1); a Recent row of an open compare switches to its tab (TN2).

use std::sync::Arc;
use std::time::Duration;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use navo_compare_cli::args::SideSpec;
use navo_compare_profile::session::Session;
use navo_compare_profile::session_store::SessionStore;
use sidewise_ui::browser::CompareUpdate;

use crate::compare_tab_script::{Held, SharedSink, install_scripted, rows_script};
use crate::compare_tab_tests::TempDir;
use crate::screens::compare_tab::install_test_hooks;
use crate::shell::Shell;
use crate::start::StartEvent;
use crate::start_tabs::StartChoice;
use crate::test_window::open;

/// Two folders to compare and a profile folder for the session book.
struct Folders {
    left: TempDir,
    right: TempDir,
    profile: TempDir,
}

impl Folders {
    /// Makes the three folders.
    fn new(tag: &str) -> Self {
        Self {
            left: TempDir::new(&format!("{tag}-l")),
            right: TempDir::new(&format!("{tag}-r")),
            profile: TempDir::new(&format!("{tag}-p")),
        }
    }

    /// The sessions the store holds.
    fn stored(&self) -> Vec<Session> {
        SessionStore::open(self.profile.path().join("sessions"))
            .list()
            .expect("list")
    }
}

/// Opens a shell over a session book and a compare engine that sends nothing by itself;
/// the returned sink delivers updates to the newest run.
fn held_shell(
    cx: &mut TestAppContext,
    folders: &Folders,
) -> ((AnyWindowHandle, Entity<Shell>), SharedSink) {
    let sink = SharedSink::default();
    install_scripted(cx, Vec::new());
    cx.update(|cx| {
        install_test_hooks(Arc::new(Held::new(Arc::clone(&sink))), cx);
        Shell::install_sessions(Some(folders.profile.path()), cx).expect("book");
    });
    let opened = open(cx, |window, cx| {
        let shell = Shell::new(window, cx);
        shell.sync_rows(cx);
        shell
    });
    cx.update_window(opened.0, |_, window, cx| window.render_frame(cx))
        .expect("window");
    (opened, sink)
}

/// Makes the active start screen emit `event`, then settles.
fn emit(
    cx: &mut TestAppContext,
    (window, shell): &(AnyWindowHandle, Entity<Shell>),
    event: StartEvent,
) {
    cx.update_window(*window, |_, window, cx| {
        let screen = shell.read(cx).active_screen().cloned().expect("start tab");
        screen.update(cx, |_, cx| cx.emit(event));
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
}

/// Makes the active start screen ask for a compare of the two folders, then settles.
fn compare(cx: &mut TestAppContext, opened: &(AnyWindowHandle, Entity<Shell>), f: &Folders) {
    let event = StartEvent::Compare {
        left: SideSpec::Path(f.left.path().to_path_buf()),
        right: SideSpec::Path(f.right.path().to_path_buf()),
        choice: StartChoice::default(),
    };
    emit(cx, opened, event);
}

/// Sends `update` through the newest run's sink and lets the tab's wake pump apply it.
fn send(cx: &mut TestAppContext, sink: &SharedSink, update: CompareUpdate) {
    let sender = sink
        .lock()
        .expect("sink lock")
        .clone()
        .expect("a run started");
    sender(update);
    cx.executor().advance_clock(Duration::from_millis(20));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn a_compare_whose_side_fails_to_open_is_not_in_recent(cx: &mut TestAppContext) {
    let folders = Folders::new("tn1-fail");
    let (opened, sink) = held_shell(cx, &folders);
    compare(cx, &opened, &folders);
    send(
        cx,
        &sink,
        CompareUpdate::Failed("Repository not found".into()),
    );
    let shell = &opened.1;
    assert_eq!(shell.read_with(cx, |shell, _| shell.tab_count()), 1);
    assert!(shell.read_with(cx, |shell, _| shell.active_compare().is_some()));
    assert!(
        folders.stored().is_empty(),
        "a failed compare entered Recent"
    );
}

#[gpui_kit::test]
fn a_compare_enters_recent_when_its_run_opened_its_sides(cx: &mut TestAppContext) {
    let folders = Folders::new("tn1-ok");
    let (opened, sink) = held_shell(cx, &folders);
    compare(cx, &opened, &folders);
    assert!(
        folders.stored().is_empty(),
        "recorded before the sides opened"
    );
    for update in rows_script() {
        send(cx, &sink, update);
    }
    assert_eq!(folders.stored().len(), 1);
    let sidebar = opened.1.read_with(cx, |shell, _| shell.sidebar().clone());
    let rows = sidebar.read_with(cx, |sidebar, _| sidebar.rows().recent.len());
    assert_eq!(rows, 1, "the sidebar lists it");
}

#[gpui_kit::test]
fn f5_on_a_tab_that_now_opens_records_it(cx: &mut TestAppContext) {
    let folders = Folders::new("tn1-f5");
    let (opened, sink) = held_shell(cx, &folders);
    compare(cx, &opened, &folders);
    send(
        cx,
        &sink,
        CompareUpdate::Failed("Repository not found".into()),
    );
    assert!(folders.stored().is_empty());
    cx.simulate_keystrokes(opened.0, "f5");
    for update in rows_script() {
        send(cx, &sink, update);
    }
    assert_eq!(
        folders.stored().len(),
        1,
        "the retry that opened was not recorded"
    );
}

#[gpui_kit::test]
fn a_recent_row_of_an_open_compare_switches_to_its_tab(cx: &mut TestAppContext) {
    let folders = Folders::new("tn2");
    let (opened, sink) = held_shell(cx, &folders);
    compare(cx, &opened, &folders);
    for update in rows_script() {
        send(cx, &sink, update);
    }
    let (window, shell) = (opened.0, &opened.1);
    cx.update_window(window, |_, window, cx| {
        shell.update(cx, |shell, cx| shell.new_tab(window, cx));
    })
    .expect("window");
    assert_eq!(shell.read_with(cx, |shell, _| shell.active()), 1);
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar().clone());
    let id = sidebar.read_with(cx, |sidebar, _| sidebar.rows().recent[0].id.clone());
    let before = folders.stored()[0].opened;
    emit(cx, &opened, StartEvent::Open { id });
    assert_eq!(shell.read_with(cx, |shell, _| shell.tab_count()), 2);
    assert_eq!(
        shell.read_with(cx, |shell, _| shell.compare_tabs().len()),
        1
    );
    assert_eq!(shell.read_with(cx, |shell, _| shell.active()), 0);
    assert_eq!(
        folders.stored()[0].opened,
        before,
        "nothing is recorded again"
    );
}
