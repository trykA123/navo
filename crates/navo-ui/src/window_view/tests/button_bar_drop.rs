use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, ExternalPaths, FileDropEvent, InputEvent as _,
    Modifiers, MouseButton, Pixels, Point, TestAppContext, point, px,
};
use navo_core::{Location, PaneId};
use navo_fs::FakeFs;
use navo_icons::spec::{Shape, Tone};

use super::{NavoWindow, fake, home_beside_dest, open_options, options_for, settle};
use crate::settings::{Change, Field, Value, Values};
use crate::test_pointer::{self, dispatch, moved, up};
use crate::window_view::{
    BarIcon, ButtonAction, ButtonBarHooks, ButtonBarNews, ButtonBarSender, ButtonItem, OpenHooks,
    ProgramId, SettingsHooks, StartOptions, UserCommandHooks, UserCommandInfo, UserCommandNews,
    UserCommandSender, UserInvocation,
};

type UserCalls = Arc<Mutex<Vec<(String, UserInvocation)>>>;
type ProgramCalls = Arc<Mutex<Vec<(ProgramId, UserInvocation)>>>;

struct DropRig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    bar_tx: ButtonBarSender,
    user_tx: UserCommandSender,
    user_calls: UserCalls,
    program_calls: ProgramCalls,
    fs: Arc<FakeFs>,
    home_pane: PaneId,
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

fn open_drop_rig(cx: &mut TestAppContext, refuse_network: bool) -> DropRig {
    let fs = fake();
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

    let refuse_tx: Arc<Mutex<Option<UserCommandSender>>> = Arc::default();
    let refuse_slot = Arc::clone(&refuse_tx);
    let (user_hooks, user_tx) = UserCommandHooks::channel(
        Arc::new(move |name, invocation| {
            let is_network = invocation
                .marked
                .iter()
                .any(|loc| !loc.is_local() || loc.as_str().starts_with(r"\\"));
            user_log
                .lock()
                .expect("lock")
                .push((name.to_owned(), invocation));
            if refuse_network
                && is_network
                && let Some(tx) = refuse_slot.lock().expect("lock").as_ref()
            {
                let _ = tx.send(UserCommandNews::Refused {
                    name: name.to_owned(),
                    message: navo_l10n::tr("user-command-not-local"),
                });
            }
        }),
        None,
    );
    *refuse_tx.lock().expect("lock") = Some(user_tx.clone());

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
    let home_pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    DropRig {
        window,
        view,
        bar_tx,
        user_tx,
        user_calls,
        program_calls,
        fs,
        home_pane,
    }
}

impl DropRig {
    fn load_bar(&self, cx: &mut TestAppContext, items: Vec<ButtonItem>) {
        assert!(self.bar_tx.send(ButtonBarNews::Applied {
            items,
            warnings: Vec::new(),
        }));
        settle(cx, &self.view);
    }

    fn load_commands(&self, cx: &mut TestAppContext, cmds: Vec<UserCommandInfo>) {
        assert!(
            self.user_tx
                .send(UserCommandNews::Applied(cmds, Vec::new()))
        );
        settle(cx, &self.view);
    }

    fn grip(&self, cx: &mut TestAppContext, row: usize) -> Point<Pixels> {
        let pane = self.home_pane.get();
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            let bounds = window
                .within(("list", pane))
                .within(("row", row))
                .find("grip")
                .bounds();
            point(bounds.right() - px(4.), bounds.center().y)
        })
        .expect("grip")
    }

    fn button_centre(&self, cx: &mut TestAppContext, index: usize) -> Point<Pixels> {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window
                .within("button-bar")
                .find(("button-bar-item", index))
                .bounds()
                .center()
        })
        .expect("button centre")
    }

    fn drag_row_to(&self, cx: &mut TestAppContext, row: usize, to: Point<Pixels>) {
        let from = self.grip(cx, row);
        let nudge = point(from.x + px(12.), from.y);
        let mods = Modifiers::default();
        test_pointer::drag(cx, self.window, (MouseButton::Left, mods), &[from, nudge]);
        dispatch(cx, self.window, moved(Some(MouseButton::Left), to, mods));
    }

    fn drop_row_on(&self, cx: &mut TestAppContext, row: usize, to: Point<Pixels>) {
        self.drag_row_to(cx, row, to);
        dispatch(
            cx,
            self.window,
            up(MouseButton::Left, to, Modifiers::default()),
        );
        settle(cx, &self.view);
    }

    fn drop_external_on(&self, cx: &mut TestAppContext, path: &str, to: Point<Pixels>) {
        dispatch(
            cx,
            self.window,
            FileDropEvent::Entered {
                position: to,
                paths: ExternalPaths([PathBuf::from(path)].into_iter().collect()),
            }
            .to_platform_input(),
        );
        dispatch(
            cx,
            self.window,
            FileDropEvent::Submit { position: to }.to_platform_input(),
        );
        settle(cx, &self.view);
    }
}

#[gpui_kit::test]
fn button_bar_drop_runs_with_dropped_files(cx: &mut TestAppContext) {
    let rig = open_drop_rig(cx, false);
    rig.load_commands(
        cx,
        vec![UserCommandInfo {
            name: "Hash".to_owned(),
            key: None,
            takes_files: true,
        }],
    );
    let prog_id = ProgramId {
        generation: 3,
        index: 0,
    };
    rig.load_bar(
        cx,
        vec![
            bar_item("Hash", ButtonAction::User("Hash".to_owned()), 1),
            bar_item(
                "Pack",
                ButtonAction::Program {
                    id: prog_id,
                    line: "zip {marked}".to_owned(),
                    takes_files: true,
                },
                2,
            ),
        ],
    );

    // Drag file row 1 (a.txt) onto User button 0; hover chip appears before release.
    let btn0 = rig.button_centre(cx, 0);
    rig.drag_row_to(cx, 1, btn0);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("button-bar-drop-chip").visible());
    })
    .expect("chip visible");
    dispatch(
        cx,
        rig.window,
        up(MouseButton::Left, btn0, Modifiers::default()),
    );
    settle(cx, &rig.view);

    {
        let calls = rig.user_calls.lock().expect("lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, "Hash");
        assert_eq!(calls[0].1.file, Some(rig.fs.location("home/a.txt")));
        assert_eq!(calls[0].1.marked, vec![rig.fs.location("home/a.txt")]);
    }

    // Drag folder row 0 (docs) onto Program button 1; folders pass as they are.
    let btn1 = rig.button_centre(cx, 1);
    rig.drop_row_on(cx, 0, btn1);
    {
        let calls = rig.program_calls.lock().expect("lock");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, prog_id);
        assert_eq!(calls[0].1.marked, vec![rig.fs.location("home/docs")]);
    }

    // Drop an external path onto User button 0.
    rig.drop_external_on(cx, "/tmp/external.txt", btn0);
    let calls = rig.user_calls.lock().expect("lock");
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[1].1.marked,
        vec![Location::new("/tmp/external.txt").expect("local path")]
    );
}

#[gpui_kit::test]
fn button_bar_drop_on_non_taker_does_nothing(cx: &mut TestAppContext) {
    let rig = open_drop_rig(cx, false);
    rig.load_commands(
        cx,
        vec![UserCommandInfo {
            name: "Status".to_owned(),
            key: None,
            takes_files: false,
        }],
    );
    rig.load_bar(
        cx,
        vec![
            bar_item("Switch", ButtonAction::Navo("pane.switch".to_owned()), 1),
            bar_item("Status", ButtonAction::User("Status".to_owned()), 2),
            bar_item(
                "Ping",
                ButtonAction::Program {
                    id: ProgramId {
                        generation: 1,
                        index: 0,
                    },
                    line: "ping".to_owned(),
                    takes_files: false,
                },
                3,
            ),
        ],
    );

    let before_pane = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    for index in 0..3 {
        let target = rig.button_centre(cx, index);
        rig.drop_row_on(cx, 1, target);
    }

    let after_pane = rig
        .view
        .read_with(cx, |view, _| view.browser.state().focused_pane());
    assert_eq!(before_pane, after_pane);
    assert!(rig.user_calls.lock().expect("lock").is_empty());
    assert!(rig.program_calls.lock().expect("lock").is_empty());
}

#[gpui_kit::test]
fn button_bar_drop_network_path_is_refused(cx: &mut TestAppContext) {
    let rig = open_drop_rig(cx, true);
    rig.load_commands(
        cx,
        vec![UserCommandInfo {
            name: "Hash".to_owned(),
            key: None,
            takes_files: true,
        }],
    );
    rig.load_bar(
        cx,
        vec![bar_item("Hash", ButtonAction::User("Hash".to_owned()), 1)],
    );

    let btn0 = rig.button_centre(cx, 0);
    rig.drop_external_on(cx, r"\\server\share\doc.txt", btn0);

    let calls = rig.user_calls.lock().expect("lock");
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].1.marked,
        vec![Location::new(r"\\server\share\doc.txt").expect("unc path")]
    );
    drop(calls);

    let shown = rig.view.read_with(cx, |view, _| {
        let notice = view.status_notice()?;
        Some((notice.text.to_string(), notice.is_error))
    });
    assert_eq!(shown, Some((navo_l10n::tr("user-command-not-local"), true)));
}
