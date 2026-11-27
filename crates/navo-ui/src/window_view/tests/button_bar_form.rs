use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, Point, Role, TestAppContext, WindowBounds,
    WindowOptions, px, size,
};
use navo_icons::spec::{Shape, Tone};

use super::super::keys::EditButtonBar;
use super::{NavoWindow, fake, home_beside_dest, options_for, press, settle, type_in};
use crate::settings::{Change, Field, Value, Values};
use crate::tokens::{self, Mode};
use crate::window_view::button_bar::view::runnable;
use crate::window_view::{
    BarEdit, BarIcon, ButtonAction, ButtonBarHooks, ButtonBarNews, ButtonBarSender, ButtonItem,
    DraftAction, ItemPath, OpenHooks, ProgramId, SettingsHooks, StartOptions, UserCommandHooks,
    UserCommandInfo, UserCommandNews, UserCommandSender,
};

type Edits = Arc<Mutex<Vec<BarEdit>>>;

struct FormRig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    bar_tx: ButtonBarSender,
    user_tx: UserCommandSender,
    edits: Edits,
    fail_edit: Arc<Mutex<Option<String>>>,
}

fn bar_item(name: &str, action: ButtonAction, line: usize) -> ButtonItem {
    ButtonItem {
        name: name.to_owned(),
        icon: BarIcon::Letters {
            letters: name.chars().take(1).collect(),
            shape: Shape::Soft,
            tone: Tone::Accent,
        },
        icon_spec: None,
        action,
        line,
    }
}

fn open_form_rig(cx: &mut TestAppContext, mode: Mode, (width, height): (f32, f32)) -> FormRig {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let edits: Edits = Arc::default();
    let edits_log = Arc::clone(&edits);
    let fail_edit: Arc<Mutex<Option<String>>> = Arc::default();
    let fail_edit_hook = Arc::clone(&fail_edit);

    let (bar_hooks, bar_tx) = ButtonBarHooks::channel(
        Arc::new(|_, _| {}),
        Arc::new(move |edit| {
            if let Some(msg) = fail_edit_hook.lock().expect("fail_edit").clone() {
                return Err(msg);
            }
            edits_log.lock().expect("edits").push(edit);
            Ok(())
        }),
        Arc::new(|| Ok(())),
        None,
    );
    let (user_hooks, user_tx) = UserCommandHooks::channel(Arc::new(|_, _| {}), None);
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
            user_commands: Some(user_hooks),
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
    FormRig {
        window,
        view,
        bar_tx,
        user_tx,
        edits,
        fail_edit,
    }
}

fn load_items_and_users(
    cx: &mut TestAppContext,
    rig: &FormRig,
    items: Vec<ButtonItem>,
    users: Vec<UserCommandInfo>,
) {
    assert!(
        rig.user_tx
            .send(UserCommandNews::Applied(users, Vec::new()))
    );
    assert!(rig.bar_tx.send(ButtonBarNews::Applied {
        items,
        warnings: Vec::new(),
    }));
    settle(cx, &rig.view);
}

fn open_editor(cx: &mut TestAppContext, rig: &FormRig) {
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
fn button_bar_form_switches_kind(cx: &mut TestAppContext) {
    let rig = open_form_rig(cx, Mode::Light, (1400., 900.));
    let initial = bar_item("Refresh", ButtonAction::Navo("files.refresh".to_owned()), 1);
    let users = vec![
        UserCommandInfo {
            name: "Build".to_owned(),
            key: Some("ctrl-b".to_owned()),
            takes_files: true,
        },
        UserCommandInfo {
            name: "Deploy".to_owned(),
            key: None,
            takes_files: false,
        },
    ];
    load_items_and_users(cx, &rig, vec![initial], users);
    open_editor(cx, &rig);

    verify_user_pick(cx, &rig);
    verify_navo_pick(cx, &rig);
    verify_program_and_debounce(cx, &rig);
}

fn verify_user_pick(cx: &mut TestAppContext, rig: &FormRig) {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("button-bar-form").visible());
        window.click("button-bar-form-kind-user", cx);
        window.render_frame(cx);
        assert!(window.try_find(("button-bar-form-user", 0usize)).is_some());
        window.click(("button-bar-form-user", 0usize), cx);
    })
    .expect("pick user command");
    settle(cx, &rig.view);

    let edits = rig.edits.lock().expect("edits");
    assert_eq!(edits.len(), 1);
    assert_eq!(
        edits[0],
        BarEdit::Update {
            at: ItemPath {
                index: 0,
                child: None,
            },
            draft: crate::window_view::ItemDraft {
                name: "Refresh".to_owned(),
                action: DraftAction::User("Build".to_owned()),
                icon: None,
            },
        }
    );
}

fn verify_navo_pick(cx: &mut TestAppContext, rig: &FormRig) {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("button-bar-form-kind-navo", cx);
        window.render_frame(cx);
        window.click("button-bar-form-navo-filter", cx);
    })
    .expect("focus navo filter");
    type_in(cx, rig.window, &rig.view, "refresh");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click(("button-bar-form-navo", 0usize), cx);
    })
    .expect("pick navo command");
    settle(cx, &rig.view);

    let edits = rig.edits.lock().expect("edits");
    assert_eq!(edits.len(), 2);
    match &edits[1] {
        BarEdit::Update { draft, .. } => match &draft.action {
            DraftAction::Navo(id) => {
                assert!(runnable(id).is_some(), "expected runnable id: {id}")
            }
            other => panic!("expected Navo draft, got {other:?}"),
        },
        other => panic!("expected Update, got {other:?}"),
    }
}

fn verify_program_and_debounce(cx: &mut TestAppContext, rig: &FormRig) {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("button-bar-form-kind-program", cx);
        window.render_frame(cx);
        window.click("button-bar-form-program", cx);
    })
    .expect("switch to program tab");
    type_in(cx, rig.window, &rig.view, "tool.exe");
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert_eq!(rig.edits.lock().expect("edits").len(), 2);

    cx.update_window(rig.window, |_, window, cx| {
        window.click("button-bar-form-console-hide", cx);
    })
    .expect("toggle console hide");
    settle(cx, &rig.view);
    assert_eq!(rig.edits.lock().expect("edits").len(), 3);

    cx.update_window(rig.window, |_, window, cx| {
        window.click("button-bar-form-start-in", cx);
    })
    .expect("focus start-in");
    type_in(cx, rig.window, &rig.view, "{folder}");
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert_eq!(rig.edits.lock().expect("edits").len(), 3);
    type_in(cx, rig.window, &rig.view, "/sub");
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    assert_eq!(rig.edits.lock().expect("edits").len(), 3);
    cx.executor().advance_clock(Duration::from_millis(250));
    settle(cx, &rig.view);

    let edits = rig.edits.lock().expect("edits");
    assert_eq!(edits.len(), 4);
    assert_eq!(
        edits[3],
        BarEdit::Update {
            at: ItemPath {
                index: 0,
                child: None,
            },
            draft: crate::window_view::ItemDraft {
                name: "Refresh".to_owned(),
                action: DraftAction::Program {
                    program: "tool.exe".to_owned(),
                    args: String::new(),
                    start_in: "{folder}/sub".to_owned(),
                    hidden_console: true,
                },
                icon: None,
            },
        }
    );
}

#[gpui_kit::test]
fn button_bar_form_shows_drop_note(cx: &mut TestAppContext) {
    let rig = open_form_rig(cx, Mode::Dark, (760., 700.));
    let program = bar_item(
        "Hash",
        ButtonAction::Program {
            id: ProgramId {
                generation: 1,
                index: 0,
            },
            line: "certutil.exe".to_owned(),
            start_in: None,
            hidden_console: false,
            takes_files: false,
        },
        1,
    );
    load_items_and_users(cx, &rig, vec![program], Vec::new());
    open_editor(cx, &rig);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("button-bar-form-drop-note").is_none());
        window.click("button-bar-form-args", cx);
    })
    .expect("focus args");
    type_in(cx, rig.window, &rig.view, "{folder}");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("button-bar-form-drop-note").is_none());
    })
    .expect("folder alone hides drop note");

    press(cx, rig.window, &rig.view, "ctrl-a");
    type_in(cx, rig.window, &rig.view, "{file}");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("button-bar-form-drop-note").visible());
    })
    .expect("file reveals drop note");

    press(cx, rig.window, &rig.view, "ctrl-a");
    type_in(cx, rig.window, &rig.view, "{folder}");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("button-bar-form-drop-note").is_none());
    })
    .expect("folder hides drop note again");

    press(cx, rig.window, &rig.view, "ctrl-a");
    type_in(cx, rig.window, &rig.view, "--list {marked-list}");
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let note = window.find("button-bar-form-drop-note");
        assert!(note.visible());
    })
    .expect("marked-list reveals drop note");
}

#[gpui_kit::test]
fn button_bar_form_shows_refusal(cx: &mut TestAppContext) {
    let rig = open_form_rig(cx, Mode::Light, (1400., 900.));
    let initial = bar_item("Refresh", ButtonAction::Navo("files.refresh".to_owned()), 1);
    let users = vec![UserCommandInfo {
        name: "Build".to_owned(),
        key: None,
        takes_files: false,
    }];
    load_items_and_users(cx, &rig, vec![initial], users);
    open_editor(cx, &rig);

    *rig.fail_edit.lock().expect("fail_edit") = Some("bad command".to_owned());
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("button-bar-form-kind-user", cx);
        window.render_frame(cx);
        window.click(("button-bar-form-user", 0usize), cx);
    })
    .expect("pick user command");
    settle(cx, &rig.view);

    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        let err = window.find("button-bar-form-error");
        assert!(err.visible());
        assert_eq!(err.role(), Some(Role::Alert));
        assert_eq!(err.label(), Some("bad command"));
    })
    .expect("form refusal error visible");
}
