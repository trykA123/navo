//! Window tests for access keys: on every page, each Alt+letter reaches the action it names.

use std::sync::mpsc;

use gpui_kit::{
    AppContext, Entity, KeyDownEvent, KeyUpEvent, Keystroke, TestAppContext, VisualTestContext,
    WindowOptions,
};
use navo_ui::setup::{SetupAction, SetupCheck, Strings, access_keys};

use crate::flow::{Failure, SetupExit};
use crate::mode::{Mode, Why};
use crate::window::SetupWindow;
use crate::window_core::Shell;
use crate::window_tests::{
    exited, fresh_shell, installed, shell_in, shell_on_ready_folder, upgrade_shell,
};
use crate::window_worker::{Job, Link, Reply};

/// One open wizard window with a fake worker behind it.
struct Open {
    view: Entity<SetupWindow>,
    cx: VisualTestContext,
    inbox: mpsc::Receiver<Job>,
    _outbox: futures::channel::mpsc::UnboundedSender<Reply>,
}

/// Opens the window on `shell`, with focus where `SetupWindow::new` puts it.
fn open(cx: &mut TestAppContext, shell: Shell) -> Open {
    let (jobs, inbox) = mpsc::channel();
    let (outbox, replies) = futures::channel::mpsc::unbounded();
    let (handle, view) = cx.update(|cx| {
        gpui_kit::init(cx);
        navo_ui::tokens::apply(navo_ui::tokens::Mode::Light, cx);
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| SetupWindow::new(shell, Link { jobs, replies }, window, cx))
        })
        .unwrap()
    });
    let cx = VisualTestContext::from_window(handle, cx);
    cx.run_until_parked();
    Open {
        view,
        cx,
        inbox,
        _outbox: outbox,
    }
}

impl Open {
    /// Returns the view and the jobs sent so far, as text to compare two windows by.
    fn state(&mut self) -> String {
        let view = self.view.clone();
        let shown = self.cx.update(|_, cx| view.read(cx).shell().view());
        let jobs: Vec<Job> = self.inbox.try_iter().collect();
        format!("{shown:?} {jobs:?}")
    }
}

/// Walks a fresh install to Shortcuts, Installing or a runner result on the Installing page.
fn installing(result: Option<SetupExit>) -> Shell {
    let mut shell = shell_on_ready_folder();
    shell.act(SetupAction::Next);
    if let Some(result) = result {
        shell.act(SetupAction::Primary);
        shell.reply(exited(result));
    }
    shell
}

/// Walks an upgrade to Install and has the runner report a running Navo.
fn blocked() -> Shell {
    let mut shell = upgrade_shell();
    shell.act(SetupAction::Next);
    shell.act(SetupAction::Toggle(SetupCheck::AcceptLicence));
    shell.act(SetupAction::Next);
    shell.act(SetupAction::Primary);
    shell.reply(exited(SetupExit::Blocked));
    shell
}

/// Builds one shell per page, from Welcome through every refusal page.
fn pages() -> Vec<fn() -> Shell> {
    vec![
        fresh_shell,
        || {
            let mut shell = fresh_shell();
            shell.act(SetupAction::Next);
            shell
        },
        shell_on_ready_folder,
        || installing(None),
        || installing(Some(SetupExit::Failed(Failure::EmptyRoot(Some(1))))),
        blocked,
        || shell_in(Mode::Same(installed())),
        || shell_in(Mode::Newer(installed())),
        || shell_in(Mode::Ambiguous(Why::MultipleInstalls)),
        || installing(Some(SetupExit::Elevated)),
        || installing(Some(SetupExit::NoPayload)),
    ]
}

#[gpui_kit::test]
fn every_page_access_key_runs_its_action(cx: &mut TestAppContext) {
    let strings = Strings::load();
    for page in pages() {
        let keys = access_keys(&page().view(), &strings);
        let name = format!("{:?}", page().view().page);
        assert!(!keys.is_empty(), "{name} has no access key to test");
        for (letter, action) in keys {
            if matches!(
                action,
                SetupAction::Browse | SetupAction::OpenFolder | SetupAction::CopyDetails
            ) {
                continue;
            }
            let mut pressed = open(cx, page());
            let before = pressed.state();
            // The test harness needs a 'static chord here; one leaked string per key is fine in a test.
            let chord: &'static str = Box::leak(format!("alt-{letter}").into_boxed_str());
            let keystroke = Keystroke::parse(chord).unwrap();
            pressed.cx.simulate_event(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            pressed.cx.simulate_event(KeyUpEvent { keystroke });
            pressed.cx.run_until_parked();
            let mut acted = open(cx, page());
            acted.state();
            let view = acted.view.clone();
            acted
                .cx
                .update(|_, cx| view.update(cx, |view, cx| view.act(action, cx)));
            acted.cx.run_until_parked();
            let expected = acted.state();
            assert_ne!(
                before, expected,
                "{name}: {action:?} changes nothing to test"
            );
            assert_eq!(
                pressed.state(),
                expected,
                "{name}: Alt+{letter} did not run {action:?}"
            );
        }
    }
}
