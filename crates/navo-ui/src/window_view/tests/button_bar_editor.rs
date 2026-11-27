use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, Modifiers, MouseButton, Point,
    TestAppContext, WindowBounds, WindowOptions, point, px, size,
};
use navo_icons::spec::{Shape, Tone};

use super::super::keys::EditButtonBar;
use super::{NavoWindow, fake, home_beside_dest, options_for, press, settle, type_in};
use crate::settings::{Change, Field, Value, Values};
use crate::tokens::{self, Mode};
use crate::window_view::{
    BarEdit, BarIcon, ButtonAction, ButtonBarHooks, ButtonBarNews, ButtonBarSender, ButtonItem,
    DraftAction, ItemPath, OpenHooks, SettingsHooks, StartOptions,
};

type Edits = Arc<Mutex<Vec<BarEdit>>>;

struct EditorRig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    bar_tx: ButtonBarSender,
    edits: Edits,
    undo_calls: Arc<AtomicUsize>,
    fail_undo: Arc<Mutex<Option<String>>>,
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

fn open_editor_rig(cx: &mut TestAppContext, mode: Mode, (width, height): (f32, f32)) -> EditorRig {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let edits: Edits = Arc::default();
    let edits_log = Arc::clone(&edits);
    let undo_calls = Arc::new(AtomicUsize::new(0));
    let undo_log = Arc::clone(&undo_calls);
    let fail_undo: Arc<Mutex<Option<String>>> = Arc::default();
    let fail_undo_hook = Arc::clone(&fail_undo);

    let (bar_hooks, bar_tx) = ButtonBarHooks::channel(
        Arc::new(|_, _| {}),
        Arc::new(move |edit| {
            edits_log.lock().expect("edits").push(edit);
            Ok(())
        }),
        Arc::new(move || {
            undo_log.fetch_add(1, Ordering::Relaxed);
            if let Some(msg) = fail_undo_hook.lock().expect("fail_undo").clone() {
                return Err(msg);
            }
            Ok(())
        }),
        None,
    );
    let values = Values::default().with(&Change {
        field: Field::WindowButtonBar,
        value: Value::Flag(true),
    });
    let options = StartOptions {
        mode,
        settings: SettingsHooks {
            values,
            ..SettingsHooks::none()
        },
        open: OpenHooks {
            button_bar: Some(bar_hooks),
            ..OpenHooks::none()
        },
        ..options_for(fs, state)
    };
    let (window, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        tokens::apply(mode, cx);
        crate::window_view::bind_app_keys(cx);
        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: Point::default(),
                size: size(px(width), px(height)),
            })),
            ..Default::default()
        };
        gpui_kit::open_window(window_options, cx, |window, cx| {
            cx.new(|cx| NavoWindow::new(options, window, cx))
        })
        .expect("open window")
    });
    settle(cx, &view);
    EditorRig {
        window,
        view,
        bar_tx,
        edits,
        undo_calls,
        fail_undo,
    }
}

fn load_items(cx: &mut TestAppContext, rig: &EditorRig, items: Vec<ButtonItem>) {
    assert!(rig.bar_tx.send(ButtonBarNews::Applied {
        items,
        warnings: Vec::new(),
    }));
    settle(cx, &rig.view);
}

fn open_via_action(cx: &mut TestAppContext, rig: &EditorRig) {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(Box::new(EditButtonBar), cx);
    })
    .expect("dispatch EditButtonBar");
    settle(cx, &rig.view);
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("render editor");
}

#[gpui_kit::test]
fn button_bar_editor_moves_with_alt_arrows(cx: &mut TestAppContext) {
    let rig = open_editor_rig(cx, Mode::Light, (1400., 900.));
    let first = bar_item("First", ButtonAction::Navo("files.refresh".to_owned()), 1);
    let second = bar_item("Second", ButtonAction::Navo("pane.switch".to_owned()), 2);
    let third = bar_item("Third", ButtonAction::Navo("files.copy".to_owned()), 3);
    load_items(cx, &rig, vec![first.clone(), second.clone(), third.clone()]);

    open_via_action(cx, &rig);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("button-bar-editor").is_some());
        assert!(
            window
                .try_find(("button-bar-editor-tile", 0usize))
                .is_some()
        );
    })
    .expect("editor visible");

    press(cx, rig.window, &rig.view, "alt-right");
    assert_eq!(
        *rig.edits.lock().expect("edits"),
        vec![BarEdit::Move {
            from: ItemPath {
                index: 0,
                child: None,
            },
            to: ItemPath {
                index: 1,
                child: None,
            },
        }]
    );

    load_items(cx, &rig, vec![second, first, third]);
    press(cx, rig.window, &rig.view, "alt-left");
    let edits = rig.edits.lock().expect("edits");
    assert_eq!(edits.len(), 2);
    assert_eq!(
        edits[1],
        BarEdit::Move {
            from: ItemPath {
                index: 1,
                child: None,
            },
            to: ItemPath {
                index: 0,
                child: None,
            },
        }
    );
}

#[gpui_kit::test]
fn button_bar_editor_drag_reorders_tiles(cx: &mut TestAppContext) {
    let rig = open_editor_rig(cx, Mode::Light, (1400., 900.));
    let first = bar_item("First", ButtonAction::Navo("files.refresh".to_owned()), 1);
    let second = bar_item("Second", ButtonAction::Navo("pane.switch".to_owned()), 2);
    let third = bar_item("Third", ButtonAction::Navo("files.copy".to_owned()), 3);
    load_items(cx, &rig, vec![first, second, third]);
    open_via_action(cx, &rig);

    let (tile0, tile2) = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            (
                window
                    .find(("button-bar-editor-tile", 0usize))
                    .bounds()
                    .center(),
                window
                    .find(("button-bar-editor-tile", 2usize))
                    .bounds()
                    .center(),
            )
        })
        .expect("tile bounds");
    let nudge = point(tile0.x + px(10.), tile0.y);
    let mods = Modifiers::default();
    crate::test_pointer::drag(cx, rig.window, (MouseButton::Left, mods), &[tile0, nudge]);
    crate::test_pointer::dispatch(
        cx,
        rig.window,
        crate::test_pointer::moved(Some(MouseButton::Left), tile2, mods),
    );
    crate::test_pointer::dispatch(
        cx,
        rig.window,
        crate::test_pointer::up(MouseButton::Left, tile2, mods),
    );
    settle(cx, &rig.view);

    assert_eq!(
        *rig.edits.lock().expect("edits"),
        vec![BarEdit::Move {
            from: ItemPath {
                index: 0,
                child: None,
            },
            to: ItemPath {
                index: 2,
                child: None,
            },
        }]
    );
}

#[gpui_kit::test]
fn button_bar_editor_insert_and_remove(cx: &mut TestAppContext) {
    let rig = open_editor_rig(cx, Mode::Light, (1400., 900.));

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.within("button-bar").click("button-bar-edit", cx);
        window.render_frame(cx);
        assert!(window.try_find("button-bar-editor").is_some());
    })
    .expect("open from empty bar");
    settle(cx, &rig.view);

    press(cx, rig.window, &rig.view, "insert");
    let after_insert = rig.edits.lock().expect("edits").clone();
    assert_eq!(after_insert.len(), 1);
    assert!(matches!(
        &after_insert[0],
        BarEdit::Insert {
            at: ItemPath {
                index: 0,
                child: None
            },
            draft
        } if !draft.name.is_empty()
    ));

    load_items(
        cx,
        &rig,
        vec![bar_item(
            "New button",
            ButtonAction::Navo("files.refresh".to_owned()),
            1,
        )],
    );

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("button-bar-editor-add-separator", cx);
    })
    .expect("add separator");
    settle(cx, &rig.view);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("button-bar-editor-add-menu", cx);
    })
    .expect("add menu");
    settle(cx, &rig.view);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("button-bar-editor-add-button", cx);
    })
    .expect("add button");
    settle(cx, &rig.view);

    {
        let edits = rig.edits.lock().expect("edits");
        assert_eq!(edits.len(), 4);
        assert!(matches!(
            &edits[1],
            BarEdit::Insert { draft, .. } if matches!(draft.action, DraftAction::Separator)
        ));
        assert!(matches!(
            &edits[2],
            BarEdit::Insert { draft, .. } if matches!(draft.action, DraftAction::Menu)
        ));
        assert!(matches!(
            &edits[3],
            BarEdit::Insert { draft, .. } if matches!(draft.action, DraftAction::Navo(_))
        ));
    }

    press(cx, rig.window, &rig.view, "ctrl-d");
    press(cx, rig.window, &rig.view, "delete");
    let edits = rig.edits.lock().expect("edits");
    assert_eq!(edits.len(), 6);
    assert!(matches!(&edits[4], BarEdit::Duplicate { .. }));
    assert!(matches!(&edits[5], BarEdit::Remove { .. }));
}

#[gpui_kit::test]
fn button_bar_editor_escape_closes(cx: &mut TestAppContext) {
    let rig = open_editor_rig(cx, Mode::Light, (1400., 900.));
    let pane = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());

    open_via_action(cx, &rig);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("button-bar-editor").is_some());
    })
    .expect("editor open");

    press(cx, rig.window, &rig.view, "escape");
    let pane_focused = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.try_find("button-bar-editor").is_none());
            rig.view.read_with(cx, |view, _| {
                view.focus
                    .get(&pane)
                    .is_some_and(|handle| handle.is_focused(window))
            })
        })
        .expect("editor closed");
    assert!(pane_focused);

    press(cx, rig.window, &rig.view, "tab");
    let after = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    assert_ne!(pane, after);
}

#[gpui_kit::test]
fn button_bar_editor_undo_calls_hook(cx: &mut TestAppContext) {
    let rig = open_editor_rig(cx, Mode::Light, (1400., 900.));
    load_items(
        cx,
        &rig,
        vec![bar_item(
            "Refresh",
            ButtonAction::Navo("files.refresh".to_owned()),
            1,
        )],
    );
    open_via_action(cx, &rig);

    press(cx, rig.window, &rig.view, "ctrl-z");
    assert_eq!(rig.undo_calls.load(Ordering::Relaxed), 1);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("button-bar-editor-undo", cx);
    })
    .expect("click undo");
    settle(cx, &rig.view);
    assert_eq!(rig.undo_calls.load(Ordering::Relaxed), 2);

    *rig.fail_undo.lock().expect("fail_undo") = Some("buttonbar.toml changed outside Navo".into());
    press(cx, rig.window, &rig.view, "ctrl-z");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let err = window.find("button-bar-editor-error");
        assert!(err.visible());
        assert_eq!(err.label(), Some("buttonbar.toml changed outside Navo"));
    })
    .expect("error banner visible");
}

#[gpui_kit::test]
fn button_bar_editor_renders_responsive_in_both_themes(cx: &mut TestAppContext) {
    for mode in [Mode::Light, Mode::Dark] {
        let narrow = open_editor_rig(cx, mode, (760., 700.));
        load_items(
            cx,
            &narrow,
            vec![bar_item(
                "Refresh",
                ButtonAction::Navo("files.refresh".to_owned()),
                1,
            )],
        );
        open_via_action(cx, &narrow);
        cx.update_window(narrow.window, |_, window, cx| {
            window.render_frame(cx);
            let card = window.find("button-bar-editor");
            assert!(card.visible());
            assert!(card.bounds().right() <= px(760.));
            assert!(window.try_find("button-bar-editor-tabs").is_some());
            assert!(window.try_find("button-bar-editor-button-pane").is_some());
            assert!(window.try_find("button-bar-editor-icon-pane").is_none());
            window.click("button-bar-editor-tab-icon", cx);
            window.render_frame(cx);
            assert!(window.try_find("button-bar-editor-button-pane").is_none());
            assert!(window.try_find("button-bar-editor-icon-pane").is_some());
        })
        .expect("narrow layout");

        let wide = open_editor_rig(cx, mode, (1400., 900.));
        load_items(
            cx,
            &wide,
            vec![bar_item(
                "Refresh",
                ButtonAction::Navo("files.refresh".to_owned()),
                1,
            )],
        );
        open_via_action(cx, &wide);
        cx.update_window(wide.window, |_, window, cx| {
            window.render_frame(cx);
            let card = window.find("button-bar-editor");
            assert!(card.visible());
            assert!(card.bounds().right() <= px(1400.));
            assert!(window.try_find("button-bar-editor-tabs").is_none());
            assert!(window.try_find("button-bar-editor-split").is_some());
            assert!(window.try_find("button-bar-editor-button-pane").is_some());
            assert!(window.try_find("button-bar-editor-icon-pane").is_some());
        })
        .expect("wide layout");
    }
}

#[gpui_kit::test]
fn button_bar_editor_opens_from_context_menu_selecting_clicked_button(cx: &mut TestAppContext) {
    let rig = open_editor_rig(cx, Mode::Light, (1400., 900.));
    load_items(
        cx,
        &rig,
        vec![
            bar_item("First", ButtonAction::Navo("files.refresh".to_owned()), 1),
            bar_item("Second", ButtonAction::Navo("pane.switch".to_owned()), 2),
        ],
    );

    let button1 = cx
        .update_window(rig.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .within("button-bar")
                .find(("button-bar-item", 1usize))
                .bounds()
                .center()
        })
        .expect("button 1 bounds");
    crate::test_pointer::dispatch(
        cx,
        rig.window,
        crate::test_pointer::down(MouseButton::Right, button1, Modifiers::default()),
    );
    crate::test_pointer::dispatch(
        cx,
        rig.window,
        crate::test_pointer::up(MouseButton::Right, button1, Modifiers::default()),
    );
    cx.run_until_parked();

    type_in(cx, rig.window, &rig.view, "edit this");
    press(cx, rig.window, &rig.view, "enter");

    let selected = rig.view.read_with(cx, |view, _| {
        view.button_bar.editor.as_ref().and_then(|e| e.selected)
    });
    assert_eq!(
        selected,
        Some(ItemPath {
            index: 1,
            child: None,
        })
    );
}
