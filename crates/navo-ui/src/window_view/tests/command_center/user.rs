use std::sync::{Arc, Mutex};

use gpui_kit::TestAppContext;

use super::super::{open_options, options_for, settle, two_panes};
use super::{Rig, fake};
use crate::window_view::{
    OpenHooks, StartOptions, UserCommandHooks, UserCommandInfo, UserCommandNews, UserCommandSender,
    UserInvocation,
};

type Runs = Arc<Mutex<Vec<String>>>;

fn rig(cx: &mut TestAppContext) -> (Rig, UserCommandSender, Runs) {
    let fs = fake();
    fs.add_file("home/a.txt", 1);
    let state = two_panes(&fs);
    let runs = Runs::default();
    let seen = runs.clone();
    let run = Arc::new(move |name: &str, _: UserInvocation| {
        seen.lock().unwrap().push(name.to_owned());
    });
    let (hooks, sender) = UserCommandHooks::channel(run, None);
    let options = StartOptions {
        open: OpenHooks {
            user_commands: Some(hooks),
            ..OpenHooks::none()
        },
        ..options_for(fs, state)
    };
    let (window, view) = open_options(cx, options);
    (Rig { window, view }, sender, runs)
}

fn apply(cx: &mut TestAppContext, rig: &Rig, sender: &UserCommandSender, names: &[(&str, &str)]) {
    let list = names
        .iter()
        .map(|(name, key)| UserCommandInfo {
            name: (*name).to_owned(),
            key: Some((*key).to_owned()),
        })
        .collect();
    assert!(sender.send(UserCommandNews::Applied(list, Vec::new())));
    settle(cx, &rig.view);
}

fn search(cx: &mut TestAppContext, rig: &Rig, text: &str) -> Vec<String> {
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, text);
    rig.rows(cx)
}

#[gpui_kit::test]
fn a_user_command_shows_in_ctrl_k_and_runs(cx: &mut TestAppContext) {
    let (rig, sender, runs) = rig(cx);
    apply(cx, &rig, &sender, &[("Zipalot", "ctrl-alt-z")]);
    assert!(
        search(cx, &rig, ">zipalot")
            .iter()
            .any(|row| row.contains("Zipalot"))
    );
    rig.press(cx, "enter");
    settle(cx, &rig.view);
    assert_eq!(*runs.lock().unwrap(), ["Zipalot"]);
    assert!(!rig.is_open(cx));
}

#[gpui_kit::test]
fn a_renamed_command_updates_after_news(cx: &mut TestAppContext) {
    let (rig, sender, _) = rig(cx);
    apply(cx, &rig, &sender, &[("Zipalot", "ctrl-alt-z")]);
    apply(cx, &rig, &sender, &[("Squeezer", "ctrl-alt-z")]);
    let rows = search(cx, &rig, ">squeezer");
    assert!(rows.iter().any(|row| row.contains("Squeezer")));
    rig.press(cx, "escape");
    rig.press(cx, "escape");
    assert!(!rig.is_open(cx));
    let rows = search(cx, &rig, ">zipalot");
    assert!(!rows.iter().any(|row| row.contains("Zipalot")));
}

#[gpui_kit::test]
fn news_while_ctrl_k_is_open_updates_its_rows(cx: &mut TestAppContext) {
    let (rig, sender, _) = rig(cx);
    apply(cx, &rig, &sender, &[("Zipalot", "ctrl-alt-z")]);
    let rows = search(cx, &rig, ">z");
    assert!(rows.iter().any(|row| row.contains("Zipalot")));
    apply(cx, &rig, &sender, &[("Squeezer", "ctrl-alt-z")]);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    settle(cx, &rig.view);
    let rows = rig.rows(cx);
    assert!(rows.iter().any(|row| row.contains("Squeezer")), "{rows:?}");
    assert!(!rows.iter().any(|row| row.contains("Zipalot")), "{rows:?}");
}
