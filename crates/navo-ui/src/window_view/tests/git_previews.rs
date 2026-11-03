use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};

use super::git_menu::{Fake, behind, click_badge, is_open, open, pending_items};
use super::{press, settle};
use crate::git::{GitAction, Previewed, Shown};
use crate::window_view::NavoWindow;

const PULL_TEXT: &str = "Pull main from origin: 2 commits";

fn both() -> Previewed {
    let shown = |text: &str, id: &str| {
        Some(Shown {
            text: text.to_owned(),
            id: id.to_owned(),
        })
    };
    vec![
        (GitAction::Pull, shown(PULL_TEXT, "pull-1")),
        (
            GitAction::Push,
            shown("Push main to origin: 1 commit", "push-1"),
        ),
    ]
}

fn offers_pull(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> bool {
    view.read_with(cx, |view, _| view.git_offered().contains(&"git.pull"))
}

fn run_pull_command(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<NavoWindow>) {
    let action = cx
        .update(|cx| cx.build_action("file_list::GitPull", None))
        .expect("action");
    view.update(cx, |view, cx| {
        let pane = view.browser.state().focused_pane();
        let handle = view.handle(pane, cx);
        let _ = window.update(cx, |_, window, cx| window.focus(&handle, cx));
    });
    cx.update_window(window, |_, window, cx| window.dispatch_action(action, cx))
        .expect("window");
    settle(cx, view);
}

#[gpui_kit::test]
fn the_menu_opens_at_once_and_a_waiting_pull_runs_nothing(cx: &mut TestAppContext) {
    let git = Fake::new(pending_items(), behind());
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    assert!(is_open(cx, &view));
    assert_eq!(git.requests(), 1);
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert!(git.runs().is_empty());
}

#[gpui_kit::test]
fn a_reply_enables_the_row_and_the_run_names_its_text_and_id(cx: &mut TestAppContext) {
    let git = Fake::new(pending_items(), behind());
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    git.reply(both());
    settle(cx, &view);
    press(cx, window, &view, "enter");
    assert!(git.runs().is_empty());
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert_eq!(git.runs(), [GitAction::Pull]);
    let shown = git.log.lock().unwrap().shown.clone();
    let named = (Some(PULL_TEXT.to_owned()), Some("pull-1".to_owned()));
    assert_eq!(shown, [named]);
}

#[gpui_kit::test]
fn a_refused_preview_keeps_the_row_disabled(cx: &mut TestAppContext) {
    let git = Fake::new(pending_items(), behind());
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    git.reply(vec![(GitAction::Pull, None), (GitAction::Push, None)]);
    settle(cx, &view);
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert!(git.runs().is_empty());
    assert!(!offers_pull(cx, &view));
}

#[gpui_kit::test]
fn a_reply_for_a_closed_menu_is_ignored(cx: &mut TestAppContext) {
    let git = Fake::new(pending_items(), behind());
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    press(cx, window, &view, "escape");
    git.reply(both());
    settle(cx, &view);
    assert!(!offers_pull(cx, &view));
    run_pull_command(cx, window, &view);
    assert!(git.runs().is_empty());
}

#[gpui_kit::test]
fn a_reply_for_an_earlier_menu_is_ignored_by_the_next_one(cx: &mut TestAppContext) {
    let git = Fake::new(pending_items(), behind());
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    press(cx, window, &view, "escape");
    click_badge(cx, window, &view);
    assert_eq!(git.requests(), 2);
    git.reply(both());
    settle(cx, &view);
    assert!(!offers_pull(cx, &view));
    git.reply(both());
    settle(cx, &view);
    press(cx, window, &view, "enter");
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert_eq!(git.runs(), [GitAction::Pull]);
}

#[gpui_kit::test]
fn the_palette_offers_pull_only_after_a_reply(cx: &mut TestAppContext) {
    let git = Fake::new(pending_items(), behind());
    let (window, view) = open(cx, &git);
    assert!(!offers_pull(cx, &view));
    run_pull_command(cx, window, &view);
    assert!(git.runs().is_empty());
    assert_eq!(git.requests(), 0);
    click_badge(cx, window, &view);
    assert!(!offers_pull(cx, &view));
    git.reply(both());
    settle(cx, &view);
    press(cx, window, &view, "escape");
    assert!(offers_pull(cx, &view));
    run_pull_command(cx, window, &view);
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert_eq!(git.runs(), [GitAction::Pull]);
}

#[gpui_kit::test]
fn opening_the_menu_again_forgets_the_last_reply(cx: &mut TestAppContext) {
    let git = Fake::new(pending_items(), behind());
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    git.reply(both());
    settle(cx, &view);
    press(cx, window, &view, "escape");
    click_badge(cx, window, &view);
    assert!(!offers_pull(cx, &view));
}
