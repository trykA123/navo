//! View tests of the Start to compare tab flow: a good submit swaps the tab, a bad side toasts once.

use navo_compare_cli::args::SideSpec;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use crate::compare_tab_script::{rows_script, scripted_shell, scripted_shell_with};
use crate::screens::compare_run::RunError;
use crate::screens::compare_tab::CompareRequest;
use crate::shell::Shell;
use crate::start_tabs::StartChoice;

/// A folder made fresh under the system temp folder and removed on drop.
pub(crate) struct TempDir(PathBuf);

impl TempDir {
    /// Creates the folder from `tag`, the process id and a per-process counter, so parallel
    /// tests that share a tag never share a folder.
    pub(crate) fn new(tag: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let count = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!("sidewise-h2-{tag}-{}-{count}", std::process::id());
        let path = std::env::temp_dir().join(name);
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).expect("create temp dir");
        Self(path)
    }

    /// The folder's path.
    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    /// Removes the folder this test made.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Opens a shell on the scripted compare engine (rows `a.txt`, `b.txt`, `c.txt`, `sub`) and renders one frame.
pub(crate) fn shell(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Shell>) {
    scripted_shell(cx)
}

/// Two folders that each hold the file `name`.
fn pair(tag: &str, name: &str) -> (TempDir, TempDir) {
    let left = TempDir::new(&format!("{tag}-l"));
    let right = TempDir::new(&format!("{tag}-r"));
    for root in [&left, &right] {
        fs::write(root.path().join(name), b"one").expect("write file");
    }
    (left, right)
}

/// Types both sides into the start tab and submits; the hint checks finish first.
pub(crate) fn submit(
    cx: &mut TestAppContext,
    (window, view): &(AnyWindowHandle, Entity<Shell>),
    sides: [&Path; 2],
) {
    let (left, right) = (sides[0].to_string_lossy(), sides[1].to_string_lossy());
    cx.update_window(*window, |_, window, cx| {
        let screen = view.read(cx).active_screen().expect("start").clone();
        screen.update(cx, |screen, cx| screen.set_sides(&left, &right, window, cx));
    })
    .expect("window");
    cx.run_until_parked();
    cx.update_window(*window, |_, window, cx| {
        let screen = view.read(cx).active_screen().expect("start").clone();
        screen.update(cx, |screen, cx| screen.submit(window, cx));
    })
    .expect("window");
}

/// Waits (bounded) until the active compare tab's Folders view has its cursor on `name`.
pub(crate) fn wait_for_row(
    cx: &mut TestAppContext,
    (window, view): &(AnyWindowHandle, Entity<Shell>),
    name: &str,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        cx.run_until_parked();
        let selected = cx
            .update_window(*window, |_, window, cx| {
                window.render_frame(cx);
                let tab = view.read(cx).active_compare()?.clone();
                tab.read(cx).folders().read(cx).selected_path()
            })
            .expect("window");
        if selected.as_deref() == Some(name) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[gpui_kit::test]
fn start_submit_turns_the_tab_into_a_compare_tab_with_rows(cx: &mut TestAppContext) {
    let (left, right) = pair("ok", "a.txt");
    let opened = shell(cx);
    submit(cx, &opened, [left.path(), right.path()]);
    let view = &opened.1;
    let tabs = view.read_with(cx, |shell, _| (shell.tab_count(), shell.active()));
    assert_eq!(tabs, (1, 0));
    assert!(view.read_with(cx, |shell, _| shell.active_screen().is_none()));
    assert!(wait_for_row(cx, &opened, "a.txt"), "rows reach the view");
    let toasts = view.read_with(cx, |shell, cx| shell.toast_count(cx));
    assert_eq!(toasts, 0);
}

#[gpui_kit::test]
fn a_missing_side_keeps_start_and_shows_one_toast(cx: &mut TestAppContext) {
    let (left, right) = pair("miss", "a.txt");
    let gone = right.path().join("not-there");
    let opened = shell(cx);
    let (window, view) = &opened;
    let request = CompareRequest {
        left: SideSpec::Path(left.path().to_owned()),
        right: SideSpec::Path(gone),
        choice: StartChoice::default(),
    };
    let result = cx
        .update_window(*window, |_, window, cx| {
            view.update(cx, |shell, cx| shell.open_compare((0, request), window, cx))
        })
        .expect("window");
    assert!(matches!(result, Err(RunError::Missing(_))));
    assert!(view.read_with(cx, |shell, _| shell.active_screen().is_some()));
    assert_eq!(view.read_with(cx, |shell, cx| shell.toast_count(cx)), 1);
    assert_eq!(view.read_with(cx, |shell, _| shell.tab_count()), 1);
}

#[gpui_kit::test]
fn an_index_past_the_tabs_starts_no_walk(cx: &mut TestAppContext) {
    let (left, right) = pair("idx", "a.txt");
    let opened = shell(cx);
    let (window, view) = &opened;
    let request = CompareRequest {
        left: SideSpec::Path(left.path().to_owned()),
        right: SideSpec::Path(right.path().to_owned()),
        choice: StartChoice::default(),
    };
    let result = cx
        .update_window(*window, |_, window, cx| {
            view.update(cx, |shell, cx| shell.open_compare((5, request), window, cx))
        })
        .expect("window");
    assert!(matches!(result, Err(RunError::Location(_))));
    assert!(view.read_with(cx, |shell, _| shell.active_compare().is_none()));
    assert_eq!(view.read_with(cx, |shell, cx| shell.toast_count(cx)), 1);
}

#[gpui_kit::test]
fn closing_a_compare_tab_mid_walk_is_quiet_and_the_title_names_both_sides(cx: &mut TestAppContext) {
    let (left, right) = pair("close", "a.txt");
    let mut script = rows_script();
    script.pop();
    let opened = scripted_shell_with(cx, script);
    let (window, view) = &opened;
    cx.simulate_keystrokes(*window, "ctrl-t");
    let request = CompareRequest {
        left: SideSpec::Path(left.path().to_owned()),
        right: SideSpec::Path(right.path().to_owned()),
        choice: StartChoice::default(),
    };
    let names = |dir: &TempDir| {
        dir.path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    };
    let want = format!("{} ↔ {}", names(&left), names(&right));
    cx.update_window(*window, |_, window, cx| {
        view.update(cx, |shell, cx| {
            shell.open_compare((0, request), window, cx).expect("open");
            assert_eq!(shell.tab_title(0).as_ref(), want);
            shell.close_tab(0, window, cx);
        });
    })
    .expect("window");
    for _ in 0..5 {
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(view.read_with(cx, |shell, _| shell.tab_count()), 1);
    assert!(view.read_with(cx, |shell, _| shell.active_compare().is_none()));
}
