use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use navo_core::Location;
use navo_fs::FakeFs;
use navo_icons::spec::{Shape, Tone};

use super::{NavoWindow, fake, home_beside_dest, open_options, options_for, settle};
use crate::settings::{Change, Field, Value, Values};
use crate::window_view::button_bar::MSG_UNKNOWN_COMMAND;
use crate::window_view::{
    BarIcon, ButtonAction, ButtonBarHooks, ButtonBarNews, ButtonBarSender, ButtonItem, OpenHooks,
    ProgramId, SettingsHooks, StartOptions, UserCommandHooks, UserInvocation,
};

type UserCalls = Arc<Mutex<Vec<(String, UserInvocation)>>>;
type ProgramCalls = Arc<Mutex<Vec<(ProgramId, UserInvocation)>>>;

struct RunRig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    bar_tx: ButtonBarSender,
    user_calls: UserCalls,
    program_calls: ProgramCalls,
    fs: Arc<FakeFs>,
    home: Location,
    dest: Location,
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

fn open_run_rig(cx: &mut TestAppContext) -> RunRig {
    let fs = fake();
    let home = fs.location("home");
    let dest = fs.location("dest");
    let state = home_beside_dest(&fs);
    let user_calls: UserCalls = Arc::default();
    let program_calls: ProgramCalls = Arc::default();
    let user_log = Arc::clone(&user_calls);
    let program_log = Arc::clone(&program_calls);

    let (bar_hooks, bar_tx) = ButtonBarHooks::channel(
        Arc::new(move |id, invocation| {
            program_log.lock().expect("lock").push((id, invocation));
        }),
        Arc::new(|_| Ok(())),
        Arc::new(|| Ok(())),
        None,
    );
    let (user_hooks, _user_tx) = UserCommandHooks::channel(
        Arc::new(move |name, invocation| {
            user_log
                .lock()
                .expect("lock")
                .push((name.to_owned(), invocation));
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
        ..options_for(Arc::clone(&fs), state)
    };
    let (window, view) = open_options(cx, options);
    RunRig {
        window,
        view,
        bar_tx,
        user_calls,
        program_calls,
        fs,
        home,
        dest,
    }
}

fn load_items(cx: &mut TestAppContext, rig: &RunRig, items: Vec<ButtonItem>) {
    assert!(rig.bar_tx.send(ButtonBarNews::Applied {
        items,
        warnings: Vec::new(),
    }));
    settle(cx, &rig.view);
}

fn click_button(cx: &mut TestAppContext, rig: &RunRig, index: usize) {
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within("button-bar")
            .click(("button-bar-item", index), cx);
    })
    .expect("click button");
    settle(cx, &rig.view);
}

#[gpui_kit::test]
fn button_bar_run_navo_dispatches_action(cx: &mut TestAppContext) {
    let rig = open_run_rig(cx);
    load_items(
        cx,
        &rig,
        vec![bar_item(
            "Switch",
            ButtonAction::Navo("pane.switch".to_owned()),
            1,
        )],
    );

    let before = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    click_button(cx, &rig, 0);
    let after = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    assert_ne!(before, after);
}

#[gpui_kit::test]
fn button_bar_run_user_uses_hook(cx: &mut TestAppContext) {
    let rig = open_run_rig(cx);
    let user_item = bar_item("Build", ButtonAction::User("Build".to_owned()), 1);
    load_items(cx, &rig, vec![user_item.clone()]);

    click_button(cx, &rig, 0);
    {
        let calls = rig.user_calls.lock().expect("lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "Build");
        assert_eq!(calls[0].1.folder.as_ref(), Some(&rig.home));
        assert_eq!(calls[0].1.other_folder.as_ref(), Some(&rig.dest));
    }

    let first = rig.fs.location("home/a.txt");
    let second = rig.fs.location("home/docs/report.txt");
    let dropped = vec![first.clone(), second];
    let dropped_expected = dropped.clone();
    cx.update_window(rig.window, |_, window, cx| {
        rig.view.update(cx, |view, cx| {
            view.run_button(&user_item, Some(dropped), window, cx);
        });
    })
    .expect("run with drop");

    let calls = rig.user_calls.lock().expect("lock");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].0, "Build");
    assert_eq!(calls[1].1.file, Some(first));
    assert_eq!(calls[1].1.marked, dropped_expected);
    assert_eq!(calls[1].1.folder.as_ref(), Some(&rig.home));
    assert_eq!(calls[1].1.other_folder.as_ref(), Some(&rig.dest));
}

#[gpui_kit::test]
fn button_bar_run_program_passes_generation(cx: &mut TestAppContext) {
    let rig = open_run_rig(cx);
    let expected_id = ProgramId {
        generation: 7,
        index: 2,
    };
    let program_item = bar_item(
        "Hash",
        ButtonAction::Program {
            id: expected_id,
            line: "sha256sum {file}".to_owned(),
            start_in: None,
            hidden_console: false,
            takes_files: true,
        },
        1,
    );
    load_items(cx, &rig, vec![program_item.clone()]);

    click_button(cx, &rig, 0);
    {
        let calls = rig.program_calls.lock().expect("lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, expected_id);
        assert_eq!(calls[0].1.folder.as_ref(), Some(&rig.home));
        assert_eq!(calls[0].1.other_folder.as_ref(), Some(&rig.dest));
    }

    let dropped_one = rig.fs.location("home/a.txt");
    let dropped = vec![dropped_one.clone()];
    cx.update_window(rig.window, |_, window, cx| {
        rig.view.update(cx, |view, cx| {
            view.run_button(&program_item, Some(dropped.clone()), window, cx);
        });
    })
    .expect("run program with drop");

    let calls = rig.program_calls.lock().expect("lock");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[1].0, expected_id);
    assert_eq!(calls[1].1.file, Some(dropped_one));
    assert_eq!(calls[1].1.marked, dropped);
    assert_eq!(calls[1].1.folder.as_ref(), Some(&rig.home));
}

#[gpui_kit::test]
fn button_bar_run_unknown_navo_shows_notice(cx: &mut TestAppContext) {
    let rig = open_run_rig(cx);
    load_items(
        cx,
        &rig,
        vec![bar_item(
            "Ghost",
            ButtonAction::Navo("nope.missing".to_owned()),
            1,
        )],
    );

    click_button(cx, &rig, 0);
    let shown = rig.view.read_with(cx, |view, _| {
        let notice = view.status_notice()?;
        Some((notice.text.to_string(), notice.is_error))
    });
    assert_eq!(shown, Some((navo_l10n::tr(MSG_UNKNOWN_COMMAND), true)));
}
