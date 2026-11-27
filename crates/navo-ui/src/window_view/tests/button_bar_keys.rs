use std::sync::{Arc, Mutex};

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use navo_icons::spec::{Shape, Tone};

use super::{NavoWindow, fake, home_beside_dest, open_options, options_for, press, settle};
use crate::settings::{Change, Field, Value, Values};
use crate::window_view::{
    BarIcon, ButtonAction, ButtonBarHooks, ButtonBarNews, ButtonBarSender, ButtonItem, OpenHooks,
    SettingsHooks, StartOptions, UserCommandHooks,
};

type UserCalls = Arc<Mutex<Vec<String>>>;

struct KeysRig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    bar_tx: ButtonBarSender,
    user_calls: UserCalls,
}

fn bar_item(name: &str, action: ButtonAction, line: usize) -> ButtonItem {
    ButtonItem {
        name: name.to_owned(),
        icon: BarIcon::Letters {
            letters: name.chars().take(1).collect(),
            shape: Shape::Soft,
            tone: Tone::Accent,
        },
        action,
        line,
    }
}

fn separator(line: usize) -> ButtonItem {
    ButtonItem {
        name: String::new(),
        icon: BarIcon::Missing { dashed: false },
        action: ButtonAction::Separator,
        line,
    }
}

fn open_keys_rig(cx: &mut TestAppContext) -> KeysRig {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let user_calls: UserCalls = Arc::default();
    let user_log = Arc::clone(&user_calls);

    let (bar_hooks, bar_tx) = ButtonBarHooks::channel(
        Arc::new(|_, _| {}),
        Arc::new(|_| Ok(())),
        Arc::new(|| Ok(())),
        None,
    );
    let (user_hooks, _user_tx) = UserCommandHooks::channel(
        Arc::new(move |name, _| {
            user_log.lock().expect("lock").push(name.to_owned());
        }),
        None,
    );
    let values = Values::default().with(&Change {
        field: Field::WindowButtonBar,
        value: Value::Flag(true),
    });
    let options = StartOptions {
        settings: SettingsHooks {
            values,
            ..SettingsHooks::none()
        },
        open: OpenHooks {
            button_bar: Some(bar_hooks),
            user_commands: Some(user_hooks),
            ..OpenHooks::none()
        },
        ..options_for(fs, state)
    };
    let (window, view) = open_options(cx, options);
    KeysRig {
        window,
        view,
        bar_tx,
        user_calls,
    }
}

fn load_items(cx: &mut TestAppContext, rig: &KeysRig, items: Vec<ButtonItem>) {
    assert!(rig.bar_tx.send(ButtonBarNews::Applied {
        items,
        warnings: Vec::new(),
    }));
    settle(cx, &rig.view);
}

#[gpui_kit::test]
fn button_bar_keys_move_and_run(cx: &mut TestAppContext) {
    let rig = open_keys_rig(cx);
    load_items(
        cx,
        &rig,
        vec![
            bar_item("First", ButtonAction::User("First".to_owned()), 1),
            separator(2),
            bar_item("Second", ButtonAction::User("Second".to_owned()), 3),
        ],
    );

    press(cx, rig.window, &rig.view, "ctrl-shift-b");
    assert_eq!(
        rig.view.read_with(cx, |view, _| view.button_bar.focused),
        Some(0)
    );

    press(cx, rig.window, &rig.view, "right");
    assert_eq!(
        rig.view.read_with(cx, |view, _| view.button_bar.focused),
        Some(2)
    );

    press(cx, rig.window, &rig.view, "right");
    assert_eq!(
        rig.view.read_with(cx, |view, _| view.button_bar.focused),
        Some(0)
    );

    press(cx, rig.window, &rig.view, "left");
    assert_eq!(
        rig.view.read_with(cx, |view, _| view.button_bar.focused),
        Some(2)
    );

    press(cx, rig.window, &rig.view, "enter");
    assert_eq!(*rig.user_calls.lock().expect("lock"), vec!["Second"]);
}

#[gpui_kit::test]
fn button_bar_keys_escape_returns_to_pane(cx: &mut TestAppContext) {
    let rig = open_keys_rig(cx);
    load_items(
        cx,
        &rig,
        vec![
            bar_item("First", ButtonAction::User("First".to_owned()), 1),
            bar_item("Second", ButtonAction::User("Second".to_owned()), 2),
        ],
    );

    let pane = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());

    press(cx, rig.window, &rig.view, "ctrl-shift-b");
    assert_eq!(
        rig.view.read_with(cx, |view, _| view.button_bar.focused),
        Some(0)
    );

    press(cx, rig.window, &rig.view, "escape");
    assert_eq!(
        rig.view.read_with(cx, |view, _| view.button_bar.focused),
        None
    );

    let pane_focused = cx
        .update_window(rig.window, |_, window, cx| {
            rig.view.read_with(cx, |view, _| {
                view.focus
                    .get(&pane)
                    .is_some_and(|handle| handle.is_focused(window))
            })
        })
        .expect("window");
    assert!(pane_focused);

    press(cx, rig.window, &rig.view, "tab");
    let after = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    assert_ne!(pane, after);
}
