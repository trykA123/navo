use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext};
use rusty_core::{AppState, FsErrorKind, PaneId};
use rusty_fs::{FakeFs, FakeOp, Fault};

use super::super::FerriteWindow;
use super::{open_with, press, settle};

const WITHIN: Duration = Duration::from_secs(5);

fn fake(watching: bool) -> Arc<FakeFs> {
    let fs = if watching {
        FakeFs::new().with_watch()
    } else {
        FakeFs::new()
    };
    for name in ["b.txt", "c.txt", "d.txt"] {
        fs.add_file(&format!("home/{name}"), 1);
    }
    Arc::new(fs)
}

struct Open {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    pane: PaneId,
}

fn open(cx: &mut TestAppContext, fs: &Arc<FakeFs>) -> Open {
    let state = AppState::new(fs.location("home"));
    let (window, view) = open_with(cx, fs.clone(), state, Rc::default());
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    Open { window, view, pane }
}

fn row_labels(cx: &mut TestAppContext, open: &Open) -> Vec<String> {
    cx.update_window(open.window, |_, window, cx| {
        window.render_frame(cx);
        let list = window.within(("list", open.pane.get()));
        (0..8usize)
            .filter_map(|row| list.try_find(("row", row)))
            .filter_map(|row| row.label().map(str::to_owned))
            .collect()
    })
    .expect("window")
}

fn shows(labels: &[String], name: &str) -> bool {
    labels.iter().any(|label| label.starts_with(name))
}

fn wait_for_row(cx: &mut TestAppContext, open: &Open, name: &str) -> bool {
    let deadline = Instant::now() + WITHIN;
    while Instant::now() < deadline {
        settle(cx, &open.view);
        if shows(&row_labels(cx, open), name) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    false
}

fn cursor_name(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Option<String> {
    view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab();
        tab.listing()
            .and_then(|l| l.get(tab.view().cursor))
            .map(|e| e.name.clone())
    })
}

#[gpui_kit::test]
fn a_file_made_on_disk_shows_in_the_open_pane_on_its_own(cx: &mut TestAppContext) {
    let fs = fake(true);
    let open = open(cx, &fs);
    press(cx, open.window, &open.view, "down");
    assert_eq!(cursor_name(cx, &open.view).as_deref(), Some("c.txt"));
    fs.add_file("home/a.txt", 1);
    assert!(wait_for_row(cx, &open, "a.txt"));
    assert_eq!(cursor_name(cx, &open.view).as_deref(), Some("c.txt"));
}

#[gpui_kit::test]
fn ctrl_r_lists_the_active_pane_again(cx: &mut TestAppContext) {
    let fs = fake(false);
    let open = open(cx, &fs);
    fs.add_file("home/a.txt", 1);
    let listed = fs.calls("home", FakeOp::List);
    assert!(!shows(&row_labels(cx, &open), "a.txt"));
    press(cx, open.window, &open.view, "ctrl-r");
    assert_eq!(fs.calls("home", FakeOp::List), listed + 1);
    assert!(shows(&row_labels(cx, &open), "a.txt"));
}

#[gpui_kit::test]
fn activating_the_window_revalidates_a_folder_without_a_watcher(cx: &mut TestAppContext) {
    let fs = fake(true);
    fs.fault(
        "home",
        FakeOp::Watch,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let open = open(cx, &fs);
    fs.add_file("home/a.txt", 1);
    let mut visual = VisualTestContext::from_window(open.window, cx);
    visual.deactivate_window();
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    assert!(wait_for_row(cx, &open, "a.txt"));
}
