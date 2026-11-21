//! View tests of the shell opening Git compares: a Start event, the Recent row, and reopening it.

use std::fs;

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use navo_compare_cli::args::SideSpec;
use sidewise_git::fixture::Fixture;

use crate::compare_tab_script::{install_scripted, rows_script};
use crate::shell::Shell;
use crate::start::StartEvent;
use crate::start_tabs::StartChoice;
use crate::test_window::open;

/// Opens a shell over a session book in `profile` and renders one frame.
fn open_shell(
    cx: &mut TestAppContext,
    profile: &std::path::Path,
) -> (AnyWindowHandle, Entity<Shell>) {
    install_scripted(cx, rows_script());
    cx.update(|cx| Shell::install_sessions(Some(profile), cx).expect("book"));
    let opened = open(cx, |window, cx| {
        let shell = Shell::new(window, cx);
        shell.sync_rows(cx);
        shell
    });
    cx.update_window(opened.0, |_, window, cx| window.render_frame(cx))
        .expect("window");
    opened
}

/// Makes the active start screen emit `event`, then renders and settles.
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

#[gpui_kit::test]
fn a_git_compare_opens_is_listed_and_reopens(cx: &mut TestAppContext) {
    let repo = Fixture::new();
    let _ = repo.commit("main", &[("a.txt", b"a")]);
    let folder = std::env::temp_dir().join(format!("sw-g12-{}", std::process::id()));
    fs::create_dir_all(&folder).expect("folder");
    let profile = folder.join("profile");
    let opened = open_shell(cx, &profile);
    let left = SideSpec::GitRev {
        repo: repo.path().to_path_buf(),
        rev: "main".into(),
    };
    let event = StartEvent::Compare {
        left: left.clone(),
        right: SideSpec::Path(folder.clone()),
        choice: StartChoice::default(),
    };
    emit(cx, &opened, event);
    let shell = &opened.1;
    let title = shell.read_with(cx, |shell, _| shell.tab_title(0).to_string());
    assert!(title.contains("@main ↔ "), "title was {title}");
    let sidebar = shell.read_with(cx, |shell, _| shell.sidebar().clone());
    let rows = sidebar.read_with(cx, |sidebar, _| sidebar.rows().clone());
    assert_eq!(rows.recent.len(), 1, "Recent lists the Git compare");
    cx.update_window(opened.0, |_, window, cx| {
        shell.update(cx, |shell, cx| {
            shell.new_tab(window, cx);
            shell.close_tab(0, window, cx);
        });
    })
    .expect("window");
    emit(
        cx,
        &opened,
        StartEvent::Open {
            id: rows.recent[0].id.clone(),
        },
    );
    assert_eq!(shell.read_with(cx, |shell, _| shell.tab_count()), 1);
    let again = shell.read_with(cx, |shell, _| shell.tab_title(0).to_string());
    assert_eq!(again, title, "the row reopens the same pair");
    let _ = fs::remove_dir_all(&folder);
}
