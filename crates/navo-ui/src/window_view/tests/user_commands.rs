use std::sync::Arc;

use gpui_kit::{Entity, TestAppContext};

use super::{NavoWindow, fake, open_options, options_for, settle, two_panes};
use crate::window_view::{
    OpenHooks, StartOptions, UserCommandHooks, UserCommandInfo, UserCommandNews, UserCommandSender,
};

fn info(name: &str, key: Option<&str>) -> UserCommandInfo {
    UserCommandInfo {
        name: name.to_owned(),
        key: key.map(str::to_owned),
    }
}

fn rig(cx: &mut TestAppContext) -> (Entity<NavoWindow>, UserCommandSender) {
    let fs = fake();
    fs.add_file("home/user-commands.toml", 10);
    let file = fs.location("home/user-commands.toml");
    let state = two_panes(&fs);
    let (hooks, sender) = UserCommandHooks::channel(Arc::new(|_, _| {}), Some(file));
    let options = StartOptions {
        open: OpenHooks {
            user_commands: Some(hooks),
            ..OpenHooks::none()
        },
        ..options_for(fs, state)
    };
    let (_, view) = open_options(cx, options);
    (view, sender)
}

fn list(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Vec<UserCommandInfo> {
    view.read_with(cx, |view, _| view.user_command_list().to_vec())
}

fn notice(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Option<(String, bool)> {
    view.read_with(cx, |view, _| {
        let shown = view.status_notice()?;
        Some((shown.text.to_string(), shown.is_error))
    })
}

fn send(
    cx: &mut TestAppContext,
    view: &Entity<NavoWindow>,
    tx: &UserCommandSender,
    n: UserCommandNews,
) {
    assert!(tx.send(n));
    settle(cx, view);
}

#[gpui_kit::test]
fn applied_replaces_the_list(cx: &mut TestAppContext) {
    let (view, tx) = rig(cx);
    assert!(list(cx, &view).is_empty());
    let first = vec![info("Build", Some("ctrl-b")), info("Open", None)];
    send(
        cx,
        &view,
        &tx,
        UserCommandNews::Applied(first.clone(), Vec::new()),
    );
    assert_eq!(list(cx, &view), first);
    let second = vec![info("Lint", None)];
    send(
        cx,
        &view,
        &tx,
        UserCommandNews::Applied(second.clone(), Vec::new()),
    );
    assert_eq!(list(cx, &view), second);
    assert_eq!(notice(cx, &view), None);
}

#[gpui_kit::test]
fn applied_warnings_show_as_a_plain_notice(cx: &mut TestAppContext) {
    let (view, tx) = rig(cx);
    let warnings = vec!["one".to_owned(), "two".to_owned()];
    send(
        cx,
        &view,
        &tx,
        UserCommandNews::Applied(vec![info("A", None)], warnings),
    );
    assert_eq!(notice(cx, &view), Some(("one · two".to_owned(), false)));
}

#[gpui_kit::test]
fn invalid_keeps_the_old_list_and_shows_the_line(cx: &mut TestAppContext) {
    let (view, tx) = rig(cx);
    let good = vec![info("Build", None)];
    send(
        cx,
        &view,
        &tx,
        UserCommandNews::Applied(good.clone(), Vec::new()),
    );
    let line = "user-commands.toml, line 3: missing `program`";
    send(cx, &view, &tx, UserCommandNews::Invalid(line.to_owned()));
    assert_eq!(list(cx, &view), good);
    assert_eq!(notice(cx, &view), Some((line.to_owned(), true)));
}

#[gpui_kit::test]
fn a_later_applied_clears_the_invalid_line(cx: &mut TestAppContext) {
    let (view, tx) = rig(cx);
    send(cx, &view, &tx, UserCommandNews::Invalid("bad".to_owned()));
    send(
        cx,
        &view,
        &tx,
        UserCommandNews::Applied(vec![info("A", None)], Vec::new()),
    );
    assert_eq!(notice(cx, &view), None);
}

#[gpui_kit::test]
fn a_non_zero_exit_shows_a_notice(cx: &mut TestAppContext) {
    let (view, tx) = rig(cx);
    let exited = UserCommandNews::Exited {
        name: "Build".to_owned(),
        code: Some(3),
        hidden: true,
    };
    send(cx, &view, &tx, exited);
    assert_eq!(
        notice(cx, &view),
        Some(("Build stopped with code 3.".to_owned(), true))
    );
}

#[gpui_kit::test]
fn a_clean_exit_shows_nothing(cx: &mut TestAppContext) {
    let (view, tx) = rig(cx);
    for code in [Some(0), None] {
        let name = "Build".to_owned();
        let news = UserCommandNews::Exited {
            name,
            code,
            hidden: true,
        };
        send(cx, &view, &tx, news);
    }
    assert_eq!(notice(cx, &view), None);
}

#[gpui_kit::test]
fn a_non_zero_exit_of_a_shown_console_shows_nothing(cx: &mut TestAppContext) {
    let (view, tx) = rig(cx);
    let exited = UserCommandNews::Exited {
        name: "Build".to_owned(),
        code: Some(-1_073_741_510),
        hidden: false,
    };
    send(cx, &view, &tx, exited);
    assert_eq!(notice(cx, &view), None);
}

#[gpui_kit::test]
fn refused_shows_its_message(cx: &mut TestAppContext) {
    let (view, tx) = rig(cx);
    let refused = UserCommandNews::Refused {
        name: "Build".to_owned(),
        message: "Build needs a file under the cursor.".to_owned(),
    };
    send(cx, &view, &tx, refused);
    assert_eq!(
        notice(cx, &view),
        Some(("Build needs a file under the cursor.".to_owned(), true))
    );
}
