//! Window tests for keys and the Folder field: Enter, Esc, Alt+letter, typing and Browse.

use std::path::PathBuf;
use std::sync::mpsc;

use gpui_kit::{
    AppContext, Entity, Focusable, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, TestAppContext,
    VisualTestContext, WindowOptions,
};
use navo_ui::setup::SetupAction;

use crate::flow::Page;
use crate::window::{DEBOUNCE, SetupWindow};
use crate::window_tests::{OTHER, ROOT, candidate, shell_on_ready_folder};
use crate::window_worker::{Job, Link, Reply};

struct Open {
    view: Entity<SetupWindow>,
    cx: VisualTestContext,
    inbox: mpsc::Receiver<Job>,
    _outbox: futures::channel::mpsc::UnboundedSender<Reply>,
}

/// Opens the window on a ready Folder page, with a fake worker behind it.
fn open(cx: &mut TestAppContext) -> Open {
    let (jobs, inbox) = mpsc::channel();
    let (outbox, replies) = futures::channel::mpsc::unbounded();
    let shell = shell_on_ready_folder();
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
    fn page(&mut self) -> Page {
        let view = self.view.clone();
        self.cx.update(|_, cx| view.read(cx).shell().page())
    }

    /// Presses and releases each key, as a keyboard does; a focused button clicks on release.
    fn keys(&mut self, keys: &'static str) {
        for key in keys.split(' ') {
            let keystroke = Keystroke::parse(key).unwrap();
            self.cx.simulate_event(KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            self.cx.simulate_event(KeyUpEvent { keystroke });
        }
        self.cx.run_until_parked();
    }

    fn act(&mut self, action: SetupAction) {
        let view = self.view.clone();
        self.cx
            .update(|_, cx| view.update(cx, |view, cx| view.act(action, cx)));
        self.cx.run_until_parked();
    }

    fn jobs(&self) -> Vec<Job> {
        self.inbox.try_iter().collect()
    }
}

#[gpui_kit::test]
fn enter_runs_next_and_alt_b_goes_back(cx: &mut TestAppContext) {
    let mut w = open(cx);
    w.keys("enter");
    assert_eq!(w.page(), Page::Shortcuts);
    w.keys("alt-b");
    assert_eq!(w.page(), Page::Folder);
}

#[gpui_kit::test]
fn esc_closes_the_wizard(cx: &mut TestAppContext) {
    let mut w = open(cx);
    w.keys("escape");
    assert!(w.jobs().iter().any(|job| matches!(job, Job::Close)));
}

#[gpui_kit::test]
fn esc_and_alt_c_are_ignored_while_installing(cx: &mut TestAppContext) {
    let mut w = open(cx);
    w.act(SetupAction::Next);
    w.act(SetupAction::Primary);
    assert!(matches!(w.jobs()[..], [Job::Install(_)]));
    w.keys("escape alt-c");
    assert!(w.jobs().is_empty(), "Close must not reach the worker");
    assert_eq!(w.page(), Page::Installing);
}

#[gpui_kit::test]
fn holding_alt_shows_the_access_keys(cx: &mut TestAppContext) {
    let mut w = open(cx);
    let held = |alt| Modifiers {
        alt,
        ..Modifiers::default()
    };
    w.cx.deactivate_window();
    w.cx.update(|window, _| window.activate_window());
    w.cx.run_until_parked();
    w.cx.simulate_modifiers_change(held(true));
    let view = w.view.clone();
    assert!(w.cx.update(|_, cx| view.read(cx).shell().view().show_keys));
    w.cx.simulate_modifiers_change(held(false));
    assert!(!w.cx.update(|_, cx| view.read(cx).shell().view().show_keys));
}

#[gpui_kit::test]
fn typing_in_the_field_checks_the_folder_after_the_pause(cx: &mut TestAppContext) {
    let mut w = open(cx);
    let view = w.view.clone();
    w.cx.update(|window, cx| {
        let field = view.read(cx).field.read(cx).focus_handle(cx);
        window.focus(&field, cx);
    });
    w.keys("end");
    w.cx.simulate_input("2");
    w.cx.executor().advance_clock(DEBOUNCE);
    w.cx.run_until_parked();
    let typed = format!("{ROOT}2");
    assert!(
        matches!(&w.jobs()[..], [Job::Check(c)] if *c == candidate(&typed)),
        "one check of the typed folder"
    );
    w.keys("enter");
    assert_eq!(w.page(), Page::Folder, "Enter waits for the check");
}

#[gpui_kit::test]
fn a_browsed_folder_shows_in_the_field(cx: &mut TestAppContext) {
    let mut w = open(cx);
    let view = w.view.clone();
    let parent = PathBuf::from(r"D:\Apps");
    w.cx.update(|window, cx| {
        view.update(cx, |view, cx| view.picked(parent, window, cx));
    });
    let text = w.cx.update(|_, cx| view.read(cx).field_text(cx));
    assert_eq!(text, OTHER);
    w.cx.executor().advance_clock(DEBOUNCE);
    w.cx.run_until_parked();
    let checks = w.jobs();
    assert!(
        matches!(&checks[..], [Job::Check(c)] if *c == candidate(OTHER)),
        "the picked folder is checked once, not again for the field: {checks:?}"
    );
}

#[gpui_kit::test]
fn focus_starts_on_the_primary_button_so_space_presses_it(cx: &mut TestAppContext) {
    let mut w = open(cx);
    w.keys("space");
    assert_eq!(w.page(), Page::Shortcuts, "Space clicks the focused Next");
    w.keys("enter");
    let jobs = w.jobs();
    assert!(
        matches!(&jobs[..], [Job::Install(_)]),
        "Enter on the focused Install runs it once: {jobs:?}"
    );
}
