use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use navo_core::AppState;

use super::drive_fs::{DriveFs, local};
use super::git_menu::{Fake, behind, item, menu_items, modified, notice, open};
use super::{fake, open_options, options_for, press, settle};
use crate::git::GitAction;
use crate::window_view::NavoWindow;

fn names(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Vec<&'static str> {
    view.read_with(cx, |view, _| {
        let all = view.palette_entries();
        all.iter()
            .map(|entry| entry.spec.name)
            .filter(|name| name.starts_with("git."))
            .collect()
    })
}

fn focus_list(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<NavoWindow>) {
    view.update(cx, |view, cx| {
        let pane = view.browser.state().focused_pane();
        let handle = view.handle(pane, cx);
        let _ = window.update(cx, |_, window, cx| window.focus(&handle, cx));
    });
}

fn dispatch(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
    name: &str,
) {
    focus_list(cx, window, view);
    let action = cx.update(|cx| cx.build_action(name, None)).expect("action");
    cx.update_window(window, |_, window, cx| {
        window.dispatch_action(action, cx);
    })
    .expect("window");
    settle(cx, view);
}

#[gpui_kit::test]
fn the_palette_shows_the_commands_the_menu_offers_on_the_cursor_row(cx: &mut TestAppContext) {
    let mut items = menu_items(false);
    items.push(item(GitAction::Diff, "Diff", None));
    let git = Fake::new(items, modified());
    let (_, view) = open(cx, &git);
    assert_eq!(
        names(cx, &view),
        ["git.menu", "git.commit", "git.pull", "git.log", "git.diff"]
    );
}

#[gpui_kit::test]
fn an_item_the_menu_does_not_offer_is_hidden(cx: &mut TestAppContext) {
    let git = Fake::new(vec![item(GitAction::Log, "Log", None)], modified());
    let (_, view) = open(cx, &git);
    assert_eq!(names(cx, &view), ["git.menu", "git.log"]);
}

#[gpui_kit::test]
fn a_disabled_item_is_hidden_not_shown_disabled(cx: &mut TestAppContext) {
    let git = Fake::new(menu_items(false), behind());
    let (_, view) = open(cx, &git);
    let shown = names(cx, &view);
    assert!(!shown.contains(&"git.commit"), "{shown:?}");
    assert!(shown.contains(&"git.pull"), "{shown:?}");
}

#[gpui_kit::test]
fn without_hooks_no_git_command_shows(cx: &mut TestAppContext) {
    let state = AppState::new(local(r"C:\home"));
    let mut options = options_for(fake(), state);
    options.provider = Arc::new(DriveFs::new(None));
    let (_, view) = open_options(cx, options);
    settle(cx, &view);
    assert!(names(cx, &view).is_empty());
}

#[gpui_kit::test]
fn a_pull_by_command_asks_first_and_runs_once_confirmed(cx: &mut TestAppContext) {
    let git = Fake::new(menu_items(true), behind());
    let (window, view) = open(cx, &git);
    dispatch(cx, window, &view, "file_list::GitPull");
    assert!(git.runs().is_empty());
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert_eq!(git.runs(), [GitAction::Pull]);
}

#[gpui_kit::test]
fn a_command_the_row_does_not_allow_runs_nothing(cx: &mut TestAppContext) {
    let git = Fake::new(menu_items(false), behind());
    let (window, view) = open(cx, &git);
    dispatch(cx, window, &view, "file_list::GitCommit");
    assert!(git.runs().is_empty());
    assert_eq!(notice(cx, &view), None);
}
