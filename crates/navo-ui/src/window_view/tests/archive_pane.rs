//! Archives open in the pane (NV10): Enter or a double-click on a zip lists it as a folder,
//! Backspace at the archive root returns to the folder that holds the archive, the location reads
//! as a plain path and unsafe entries say why. F5 copies entries out (NV10b); every write into
//! or out of an archive is refused with one read-only text.

use std::io::{Cursor, Write as _};
use std::path::Path;
use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, InputEvent as _, Modifiers, MouseButton,
    MouseDownEvent, MouseUpEvent, TestAppContext,
};
use navo_core::{AppState, Command, Direction, Location, ProviderPath, apply};
use navo_fs::LocalFs;

use super::{fake, open_options, options_for, press, settle};
use crate::window_view::{NavoWindow, StartOptions};

/// Writes `bundle.zip` (a `docs/` folder with one file, plus `top.txt`) and `notes.txt` into
/// `dir`; the zip sorts first, so the cursor starts on it.
fn write_fixture(dir: &Path) {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    writer.add_directory("docs/", options).expect("folder");
    writer.start_file("docs/readme.txt", options).expect("file");
    writer.write_all(b"inside").expect("write");
    writer.start_file("top.txt", options).expect("file");
    writer.write_all(b"top").expect("write");
    let bytes = writer.finish().expect("finish").into_inner();
    std::fs::write(dir.join("bundle.zip"), bytes).expect("zip");
    std::fs::write(dir.join("notes.txt"), b"notes").expect("notes");
}

/// Opens a one-pane window on the real disk at `dir`, with the fixture written into it.
fn open_on(cx: &mut TestAppContext, dir: &Path) -> (AnyWindowHandle, Entity<NavoWindow>) {
    write_fixture(dir);
    let text = dir.to_str().expect("utf-8 temp path");
    let home = Location::local(ProviderPath::from_text(text)).expect("home");
    let options = StartOptions {
        provider: Arc::new(LocalFs::new()),
        ..options_for(fake(), AppState::new(home))
    };
    open_options(cx, options)
}

/// The active tab's location text and its row names, in list order.
fn shown(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> (String, Vec<String>) {
    view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab();
        let names = tab.listing().map_or_else(Vec::new, |listing| {
            (0..listing.len())
                .filter_map(|row| listing.get(row).map(|entry| entry.name.clone()))
                .collect()
        });
        (tab.location().as_str().to_owned(), names)
    })
}

/// The name under the cursor in the active tab.
fn cursor_name(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Option<String> {
    view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab();
        tab.listing()?
            .get(tab.view().cursor)
            .map(|entry| entry.name.clone())
    })
}

/// Double-clicks list row `row` of the focused pane with the left button, then settles.
fn double_click(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
    row: usize,
) {
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let position = window
            .within(("list", pane.get()))
            .find(("row", row))
            .bounds()
            .center();
        let down = MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 2,
            first_mouse: false,
        };
        window.dispatch_event(down.to_platform_input(), cx);
        let up = MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 2,
        };
        window.dispatch_event(up.to_platform_input(), cx);
    })
    .expect("window");
    settle(cx, view);
}

#[gpui_kit::test]
fn enter_lists_a_zip_and_backspace_at_its_root_leaves(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("temp dir");
    let (window, view) = open_on(cx, dir.path());
    let (home, _) = shown(cx, &view);
    assert_eq!(cursor_name(cx, &view).as_deref(), Some("bundle.zip"));

    press(cx, window, &view, "enter");
    let (inside, names) = shown(cx, &view);
    assert!(inside.starts_with("archive://"), "{inside}");
    assert_eq!(names, ["docs", "top.txt"]);

    press(cx, window, &view, "enter");
    assert_eq!(shown(cx, &view).1, ["readme.txt"]);
    press(cx, window, &view, "backspace");
    assert_eq!(
        shown(cx, &view),
        (inside, vec!["docs".into(), "top.txt".into()])
    );

    press(cx, window, &view, "backspace");
    let (back, names) = shown(cx, &view);
    assert_eq!(back, home);
    assert_eq!(names, ["bundle.zip", "notes.txt"]);
    assert_eq!(cursor_name(cx, &view).as_deref(), Some("bundle.zip"));
}

#[gpui_kit::test]
fn a_double_click_on_a_zip_opens_it_like_enter(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("temp dir");
    let (window, view) = open_on(cx, dir.path());
    double_click(cx, window, &view, 0);
    let (inside, names) = shown(cx, &view);
    assert!(inside.starts_with("archive://"), "{inside}");
    assert_eq!(names, ["docs", "top.txt"]);
}

/// The labels of the path bar's crumbs in the focused pane, as rendered.
fn crumb_labels(cx: &mut TestAppContext, window: AnyWindowHandle, pane: u64) -> Vec<String> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let bar = window.within(("path", pane));
        (0..32usize)
            .filter_map(|at| bar.try_find(("segment", at)))
            .filter_map(|crumb| crumb.label().map(str::to_owned))
            .collect()
    })
    .expect("window")
}

#[gpui_kit::test]
fn a_folder_inside_a_zip_reads_as_a_path(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("temp dir");
    let (window, view) = open_on(cx, dir.path());
    press(cx, window, &view, "enter");
    let root = view.read_with(cx, |view, _| {
        view.browser.state().active_tab().location().clone()
    });
    press(cx, window, &view, "enter");
    let (title, location, pane) = view.read_with(cx, |view, _| {
        let state = view.browser.state();
        let tab = state.active_tab();
        (
            view.tab_title(tab).to_string(),
            tab.location().clone(),
            state.focused_pane(),
        )
    });
    assert_eq!(title, "docs");
    let text = dir.path().join("bundle.zip").join("docs");
    assert_eq!(
        navo_fs::archive::shown_path(&location).as_deref(),
        text.to_str()
    );
    let crumbs = crate::pane_chrome::path_segments(&location);
    let tail: Vec<_> = crumbs
        .iter()
        .rev()
        .take(2)
        .map(|c| c.label.to_string())
        .collect();
    assert_eq!(tail, ["docs", "bundle.zip"]);
    assert_eq!(crumbs[crumbs.len() - 2].location, root);
    let shown = crumb_labels(cx, window, pane.get());
    assert!(
        shown.ends_with(&["bundle.zip".into(), "docs".into()]),
        "{shown:?}"
    );
    assert!(!shown.iter().any(|c| c.contains(['%', '#'])), "{shown:?}");
}

#[gpui_kit::test]
fn an_unsafe_entry_shows_why(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    for name in ["../evil.txt", "fine.txt"] {
        writer.start_file(name, options).expect("file");
        writer.write_all(b"x").expect("write");
    }
    let archive = dir.path().join("unsafe.zip");
    std::fs::write(&archive, writer.finish().expect("finish").into_inner()).expect("zip");
    let (window, view) = open_on(cx, dir.path());
    let location = navo_fs::archive::location(&archive, "").expect("archive location");
    view.update(cx, |view, _| {
        view.browser
            .command(&navo_core::Command::PaneNavigate { location })
            .expect("navigate");
    });
    settle(cx, &view);
    assert_eq!(shown(cx, &view).1, ["../evil.txt", "fine.txt"]);
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    let notes = cx
        .update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let mut list = window.within(("list", pane.get()));
            [0usize, 1].map(|row| {
                let note = list.within(("row", row)).try_find("row-note");
                note.and_then(|note| note.label().map(str::to_owned))
            })
        })
        .expect("window");
    assert_eq!(
        notes,
        [
            Some("Unsafe name: it leads out of the archive (..)".to_owned()),
            None
        ]
    );
}

/// Opens two panes on the real disk: the fixture folder `dir` (focused) beside the empty folder
/// `out`, kept apart so the fixture's cursor still starts on the zip.
fn open_beside(
    cx: &mut TestAppContext,
    dir: &Path,
    out: &Path,
) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let options = beside_options(dir, out);
    open_options(cx, options)
}

/// The start options for `open_beside`: the fixture written into `dir`, and two panes on the
/// real disk with `dir` focused beside `out`.
fn beside_options(dir: &Path, out: &Path) -> StartOptions {
    write_fixture(dir);
    let local = |path: &Path| {
        let text = path.to_str().expect("utf-8 temp path");
        Location::local(ProviderPath::from_text(text)).expect("local location")
    };
    let mut state = AppState::new(local(dir));
    let home = state.focused_pane();
    let split = Command::LayoutSplit {
        direction: Direction::Right,
    };
    apply(&mut state, &split).expect("split");
    let location = local(out);
    apply(&mut state, &Command::PaneNavigate { location }).expect("out");
    for direction in Direction::ALL {
        if state.focused_pane() == home {
            break;
        }
        let _ = apply(&mut state, &Command::LayoutFocus { direction });
    }
    StartOptions {
        provider: Arc::new(LocalFs::new()),
        ..options_for(fake(), state)
    }
}

#[gpui_kit::test]
fn f5_copies_a_zip_entry_into_the_other_pane(cx: &mut TestAppContext) {
    let (dir, out) = (
        tempfile::tempdir().expect("dir"),
        tempfile::tempdir().expect("out"),
    );
    let (window, view) = open_beside(cx, dir.path(), out.path());
    press(cx, window, &view, "enter");
    press(cx, window, &view, "down");
    assert_eq!(cursor_name(cx, &view).as_deref(), Some("top.txt"));
    press(cx, window, &view, "f5");
    let kind = view.read_with(cx, |view, _| view.browser.pending().map(|p| p.plan.kind));
    assert_eq!(kind, Some(navo_core::OpKind::Copy));
    press(cx, window, &view, "enter");
    settle(cx, &view);
    let copied = std::fs::read(out.path().join("top.txt")).expect("copied file");
    assert_eq!(copied, b"top");
}

#[gpui_kit::test]
fn f8_f6_and_f7_inside_a_zip_say_read_only_and_change_nothing(cx: &mut TestAppContext) {
    let (dir, out) = (
        tempfile::tempdir().expect("dir"),
        tempfile::tempdir().expect("out"),
    );
    let (window, view) = open_beside(cx, dir.path(), out.path());
    let zip = dir.path().join("bundle.zip");
    let before = std::fs::read(&zip).expect("zip bytes");
    press(cx, window, &view, "enter");
    for key in ["f8", "f6", "f7"] {
        view.update(cx, |view, _| view.forget_clipboard_notice());
        press(cx, window, &view, key);
        let (pending, prompt, jobs, notice) = view.read_with(cx, |view, _| {
            let notice = view.clipboard_notice().map(|n| n.text.to_string());
            let jobs = view.browser.jobs().len();
            (
                view.browser.pending().is_some(),
                view.prompt.is_some(),
                jobs,
                notice,
            )
        });
        assert!(!pending && !prompt, "{key} opened a dialog");
        assert_eq!(jobs, 0, "{key} started a job");
        assert_eq!(
            notice.as_deref(),
            Some("Archives are read-only here. Copy files out with F5."),
            "{key}"
        );
    }
    assert_eq!(std::fs::read(&zip).expect("zip bytes"), before);
    assert_eq!(std::fs::read_dir(out.path()).expect("out").count(), 0);
}

mod refusals;
