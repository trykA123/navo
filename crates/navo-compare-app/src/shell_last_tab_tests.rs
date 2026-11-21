//! View tests of closing the last tab (CHR): the window stays open and keeps one New compare tab.

use navo_compare_cli::args::SideSpec;
use std::fs;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, EntityId, TestAppContext};

use crate::compare_tab_script::scripted_shell;
use crate::compare_tab_tests::TempDir;
use crate::screens::compare_tab::CompareRequest;
use crate::shell::Shell;
use crate::start_tabs::StartChoice;

/// (tab count, active is a start tab, open windows), read after the scheduler settles.
fn settled(cx: &mut TestAppContext, view: &Entity<Shell>) -> (usize, bool, usize) {
    cx.run_until_parked();
    let (count, start) = view.read_with(cx, |shell, _| {
        (shell.tab_count(), shell.active_screen().is_some())
    });
    (count, start, cx.windows().len())
}

/// The entity ids of all tabs, in order.
fn ids(cx: &mut TestAppContext, view: &Entity<Shell>) -> Vec<EntityId> {
    view.read_with(cx, |shell, _| shell.tab_ids())
}

/// Replaces the lone start tab with a compare tab over two fresh folders; returns the folders.
fn open_lone_compare(
    cx: &mut TestAppContext,
    (window, view): &(AnyWindowHandle, Entity<Shell>),
) -> [TempDir; 2] {
    let dirs = [TempDir::new("chr-l"), TempDir::new("chr-r")];
    for dir in &dirs {
        fs::write(dir.path().join("a.txt"), b"one").expect("write file");
    }
    let request = CompareRequest {
        left: SideSpec::Path(dirs[0].path().to_owned()),
        right: SideSpec::Path(dirs[1].path().to_owned()),
        choice: StartChoice::default(),
    };
    cx.update_window(*window, |_, window, cx| {
        view.update(cx, |shell, cx| shell.open_compare((0, request), window, cx))
            .expect("open");
        window.render_frame(cx);
    })
    .expect("window");
    dirs
}

/// Ctrl+W on the only tab, a compare tab, leaves one fresh New compare tab and the window open.
#[gpui_kit::test]
fn ctrl_w_on_the_lone_compare_tab_leaves_a_new_compare_tab(cx: &mut TestAppContext) {
    let s = scripted_shell(cx);
    let _dirs = open_lone_compare(cx, &s);
    assert_eq!(settled(cx, &s.1), (1, false, 1), "one compare tab before");
    cx.simulate_keystrokes(s.0, "ctrl-w");
    assert_eq!(
        settled(cx, &s.1),
        (1, true, 1),
        "one start tab, window open"
    );
    let title = s.1.read_with(cx, |shell, _| shell.tab_title(0));
    assert_eq!(title.as_ref(), "New compare");
}

/// Ctrl+W on the only tab, a New compare tab, keeps that same tab: no close, no new tab.
#[gpui_kit::test]
fn ctrl_w_on_the_lone_new_compare_tab_changes_nothing(cx: &mut TestAppContext) {
    let s = scripted_shell(cx);
    let before = ids(cx, &s.1);
    cx.simulate_keystrokes(s.0, "ctrl-w");
    assert_eq!(settled(cx, &s.1), (1, true, 1));
    assert_eq!(ids(cx, &s.1), before, "the same start tab stays");
}
