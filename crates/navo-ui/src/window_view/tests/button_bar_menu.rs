use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, MouseButton, Point, TestAppContext, px,
};
use navo_icons::spec::{Shape, Tone};

use super::{
    NavoWindow, fake, home_beside_dest, open_options, options_for, press, settle, type_in,
};
use crate::settings::{Change, Field, Value, Values};
use crate::window_view::{
    BarEdit, BarIcon, ButtonAction, ButtonBarHooks, ButtonBarNews, ButtonBarSender, ButtonItem,
    ItemPath, OpenHooks, SaveReport, SettingsHooks, StartOptions, UserCommandHooks,
};

type Edits = Arc<Mutex<Vec<BarEdit>>>;
type UserCalls = Arc<Mutex<Vec<String>>>;
type SavedChanges = Arc<Mutex<Vec<Change>>>;

struct MenuRig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    bar_tx: ButtonBarSender,
    edits: Edits,
    user_calls: UserCalls,
    changes: SavedChanges,
    stored: Arc<Mutex<Values>>,
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

fn open_menu_rig(cx: &mut TestAppContext) -> MenuRig {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let edits: Edits = Arc::default();
    let edits_log = Arc::clone(&edits);
    let user_calls: UserCalls = Arc::default();
    let user_log = Arc::clone(&user_calls);
    let changes: SavedChanges = Arc::default();
    let changes_log = Arc::clone(&changes);
    let values = Values::default().with(&Change {
        field: Field::WindowButtonBar,
        value: Value::Flag(true),
    });
    let stored = Arc::new(Mutex::new(values.clone()));
    let stored_log = Arc::clone(&stored);

    let (bar_hooks, bar_tx) = ButtonBarHooks::channel(
        Arc::new(|_, _| {}),
        Arc::new(move |edit| {
            edits_log.lock().expect("edits").push(edit);
            Ok(())
        }),
        Arc::new(|| Ok(())),
        None,
    );
    let (user_hooks, _user_tx) = UserCommandHooks::channel(
        Arc::new(move |name, _| {
            user_log.lock().expect("users").push(name.to_owned());
        }),
        None,
    );
    let options = StartOptions {
        settings: SettingsHooks {
            values,
            save: Arc::new(move |batch| {
                changes_log
                    .lock()
                    .expect("changes")
                    .extend_from_slice(batch);
                let mut current = stored_log.lock().expect("stored");
                for change in batch {
                    *current = current.with(change);
                }
                SaveReport::saved(current.clone())
            }),
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
    MenuRig {
        window,
        view,
        bar_tx,
        edits,
        user_calls,
        changes,
        stored,
    }
}

fn load_items(cx: &mut TestAppContext, rig: &MenuRig, items: Vec<ButtonItem>) {
    assert!(rig.bar_tx.send(ButtonBarNews::Applied {
        items,
        warnings: Vec::new(),
    }));
    settle(cx, &rig.view);
}

fn right_click(cx: &mut TestAppContext, window: AnyWindowHandle, at: Point<gpui_kit::Pixels>) {
    crate::test_pointer::dispatch(
        cx,
        window,
        crate::test_pointer::down(MouseButton::Right, at, Modifiers::default()),
    );
    crate::test_pointer::dispatch(
        cx,
        window,
        crate::test_pointer::up(MouseButton::Right, at, Modifiers::default()),
    );
    cx.run_until_parked();
}

fn context_menu_rows(cx: &mut TestAppContext, window: AnyWindowHandle) -> Vec<String> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        if window.try_find("context-menu").is_none() {
            return Vec::new();
        }
        let mut root = window.within("context-menu");
        if root.try_find(("menu-level", 0usize)).is_none() {
            return Vec::new();
        }
        let level = root.within(("menu-level", 0usize));
        (0usize..)
            .map_while(|index| level.try_find(("menu-row", index)))
            .map(|row| row.label().unwrap_or("-").to_owned())
            .collect()
    })
    .expect("window")
}

fn bar_edge_point(cx: &mut TestAppContext, window: AnyWindowHandle) -> Point<gpui_kit::Pixels> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let bounds = window.find("button-bar").bounds();
        gpui_kit::point(bounds.right() - px(8.), bounds.center().y)
    })
    .expect("bar bounds")
}

fn button_center_point(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    index: usize,
) -> Point<gpui_kit::Pixels> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within("button-bar")
            .find(("button-bar-item", index))
            .bounds()
            .center()
    })
    .expect("button bounds")
}

#[gpui_kit::test]
fn button_bar_menu_lists_entries(cx: &mut TestAppContext) {
    let rig = open_menu_rig(cx);
    load_items(
        cx,
        &rig,
        vec![
            bar_item("First", ButtonAction::User("First".to_owned()), 1),
            bar_item("Second", ButtonAction::User("Second".to_owned()), 2),
        ],
    );

    let bar_point = bar_edge_point(cx, rig.window);
    right_click(cx, rig.window, bar_point);
    let bar_rows = context_menu_rows(cx, rig.window);
    assert!(
        bar_rows.iter().any(|r| r == "Show names"),
        "rows: {bar_rows:?}"
    );
    assert!(
        bar_rows.iter().any(|r| r == "Hide button bar"),
        "rows: {bar_rows:?}"
    );
    assert!(
        !bar_rows.iter().any(|r| r == "Remove this button"),
        "rows: {bar_rows:?}"
    );
    assert!(
        !bar_rows.iter().any(|r| r.contains("Edit")),
        "rows: {bar_rows:?}"
    );

    press(cx, rig.window, &rig.view, "escape");

    let button_point = button_center_point(cx, rig.window, 0);
    right_click(cx, rig.window, button_point);
    let item_rows = context_menu_rows(cx, rig.window);
    assert!(
        item_rows.iter().any(|r| r == "Remove this button"),
        "rows: {item_rows:?}"
    );
    assert!(
        item_rows.iter().any(|r| r == "Show names"),
        "rows: {item_rows:?}"
    );
    assert!(
        item_rows.iter().any(|r| r == "Hide button bar"),
        "rows: {item_rows:?}"
    );

    type_in(cx, rig.window, &rig.view, "remove");
    press(cx, rig.window, &rig.view, "enter");
    assert_eq!(
        *rig.edits.lock().expect("edits"),
        vec![BarEdit::Remove {
            at: ItemPath {
                index: 0,
                child: None,
            }
        }]
    );

    press(cx, rig.window, &rig.view, "ctrl-shift-b");
    press(cx, rig.window, &rig.view, "shift-f10");
    let key_rows = context_menu_rows(cx, rig.window);
    assert!(
        key_rows.iter().any(|r| r == "Remove this button"),
        "rows: {key_rows:?}"
    );
    type_in(cx, rig.window, &rig.view, "show names");
    press(cx, rig.window, &rig.view, "enter");
    assert_eq!(
        *rig.changes.lock().expect("changes"),
        vec![Change {
            field: Field::WindowButtonBarNames,
            value: Value::Flag(true),
        }]
    );
    assert!(
        rig.stored
            .lock()
            .expect("stored")
            .flag(Field::WindowButtonBarNames)
    );
}

#[gpui_kit::test]
fn button_bar_menu_hide_turns_setting_off(cx: &mut TestAppContext) {
    let rig = open_menu_rig(cx);
    load_items(
        cx,
        &rig,
        vec![bar_item("First", ButtonAction::User("First".to_owned()), 1)],
    );

    let bar_point = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            let bounds = window.find("button-bar").bounds();
            gpui_kit::point(bounds.right() - px(8.), bounds.center().y)
        })
        .expect("bar bounds");
    right_click(cx, rig.window, bar_point);

    type_in(cx, rig.window, &rig.view, "hide");
    press(cx, rig.window, &rig.view, "enter");

    assert_eq!(
        *rig.changes.lock().expect("changes"),
        vec![Change {
            field: Field::WindowButtonBar,
            value: Value::Flag(false),
        }]
    );
    assert!(
        !rig.stored
            .lock()
            .expect("stored")
            .flag(Field::WindowButtonBar)
    );
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("button-bar").is_none());
    })
    .expect("window");
}

#[gpui_kit::test]
fn button_bar_menu_button_runs_child(cx: &mut TestAppContext) {
    let rig = open_menu_rig(cx);
    let children = vec![
        bar_item("Alpha", ButtonAction::User("Alpha".to_owned()), 2),
        bar_item("Beta", ButtonAction::User("Beta".to_owned()), 3),
    ];
    load_items(
        cx,
        &rig,
        vec![bar_item("Tools", ButtonAction::Menu(children), 1)],
    );

    press(cx, rig.window, &rig.view, "ctrl-shift-b");
    press(cx, rig.window, &rig.view, "down");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("button-bar-menu-popup").is_some());
        assert!(window.try_find(("button-bar-menu-row", 0usize)).is_some());
        assert!(window.try_find(("button-bar-menu-row", 1usize)).is_some());
    })
    .expect("popup open");

    press(cx, rig.window, &rig.view, "down");
    press(cx, rig.window, &rig.view, "enter");
    assert_eq!(*rig.user_calls.lock().expect("calls"), vec!["Beta"]);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("button-bar-menu-popup").is_none());
        window
            .within("button-bar")
            .click(("button-bar-item", 0usize), cx);
        window.render_frame(cx);
        assert!(window.try_find("button-bar-menu-popup").is_some());
        window.click(("button-bar-menu-row", 0usize), cx);
    })
    .expect("click menu child");
    settle(cx, &rig.view);

    assert_eq!(
        *rig.user_calls.lock().expect("calls"),
        vec!["Beta", "Alpha"]
    );
}
