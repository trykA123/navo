use std::sync::{Arc, Mutex};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};
use rusty_core::{Location, PaneId};

use super::super::{open_options, options_for, prompt_open, settle, two_panes};
use super::late::keys_unsettled;
use super::{Fixed, Rig, fake, hit, location};
use crate::command_center::{Group, Target};
use crate::window_view::StartOptions;

type Launched = Arc<Mutex<Vec<String>>>;

fn file(path: &str) -> Target {
    Target::Open {
        location: location(path),
        folder: false,
    }
}

fn folder(path: &str) -> Target {
    Target::Open {
        location: location(path),
        folder: true,
    }
}

fn rig_on(cx: &mut TestAppContext, target: Target) -> (Rig, Launched) {
    let fs = fake();
    fs.add_folder("home/aa");
    fs.add_file("home/docs/notes.txt", 1);
    let state = two_panes(&fs);
    let launched = Launched::default();
    let sink = launched.clone();
    let options = StartOptions {
        launcher: Arc::new(move |location: &Location| {
            sink.lock().unwrap().push(location.as_str().to_owned());
        }),
        ..options_for(fs, state)
    };
    let (window, view) = open_options(cx, options);
    let group = match &target {
        Target::Open { folder: false, .. } => Group::Files,
        _ => Group::Folders,
    };
    let source = Arc::new(Fixed(vec![hit("result", group, target)]));
    view.update(cx, |view, _| view.command_center.sources.push(source));
    (Rig { window, view }, launched)
}

fn run(cx: &mut TestAppContext, rig: &Rig, key: &str) {
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "/result");
    rig.press(cx, key);
    settle(cx, &rig.view);
}

impl Rig {
    fn focused(&self, cx: &mut TestAppContext) -> PaneId {
        self.view
            .read_with(cx, |view, _| view.browser.state().focused_pane())
    }

    fn other(&self, cx: &mut TestAppContext) -> PaneId {
        let focused = self.focused(cx);
        self.view.read_with(cx, |view, _| {
            view.panes()
                .into_iter()
                .find(|pane| *pane != focused)
                .expect("two panes")
        })
    }

    fn at(&self, cx: &mut TestAppContext, pane: PaneId) -> String {
        self.view.read_with(cx, |view, _| {
            let pane = view.browser.state().pane(pane).expect("pane");
            pane.active().location().as_str().to_owned()
        })
    }

    fn cursor(&self, cx: &mut TestAppContext) -> u64 {
        self.view.read_with(cx, |view, _| {
            view.browser.state().active_tab().view().cursor
        })
    }
}

fn here(path: &str) -> String {
    location(path).as_str().to_owned()
}

#[gpui_kit::test]
fn enter_on_a_file_launches_it_and_stays_in_the_folder(cx: &mut TestAppContext) {
    let (rig, launched) = rig_on(cx, file("home/docs/report.txt"));
    run(cx, &rig, "enter");
    assert_eq!(*launched.lock().unwrap(), [here("home/docs/report.txt")]);
    assert_eq!(rig.here(cx), here("home"));
    assert!(!rig.is_open(cx));
    assert!(rig.pane_has_focus(cx));
}

#[gpui_kit::test]
fn enter_on_a_folder_opens_it_and_launches_nothing(cx: &mut TestAppContext) {
    let (rig, launched) = rig_on(cx, folder("home/docs"));
    run(cx, &rig, "enter");
    assert!(launched.lock().unwrap().is_empty());
    assert_eq!(rig.here(cx), here("home/docs"));
    assert!(rig.pane_has_focus(cx));
}

#[gpui_kit::test]
fn shift_enter_on_a_file_shows_it_in_its_folder(cx: &mut TestAppContext) {
    let (rig, launched) = rig_on(cx, file("home/docs/report.txt"));
    run(cx, &rig, "shift-enter");
    assert!(launched.lock().unwrap().is_empty());
    assert_eq!(rig.here(cx), here("home/docs"));
    assert_eq!(rig.cursor(cx), 1);
}

#[gpui_kit::test]
fn shift_enter_on_a_folder_shows_it_in_its_parent(cx: &mut TestAppContext) {
    let (rig, _) = rig_on(cx, folder("home/docs"));
    run(cx, &rig, "shift-enter");
    assert_eq!(rig.here(cx), here("home"));
    assert_eq!(rig.cursor(cx), 1);
}

#[gpui_kit::test]
fn ctrl_enter_on_a_folder_opens_it_in_the_other_pane(cx: &mut TestAppContext) {
    let (rig, launched) = rig_on(cx, Target::GoTo(location("home/docs")));
    let (first, other) = (rig.focused(cx), rig.other(cx));
    let before = rig.at(cx, first);
    run(cx, &rig, "ctrl-enter");
    assert!(launched.lock().unwrap().is_empty());
    assert_eq!(rig.focused(cx), other);
    assert_eq!(rig.at(cx, other), here("home/docs"));
    assert_eq!(rig.at(cx, first), before);
    assert!(rig.pane_has_focus(cx));
}

#[gpui_kit::test]
fn ctrl_enter_on_a_file_shows_it_in_the_other_pane(cx: &mut TestAppContext) {
    let (rig, launched) = rig_on(cx, file("home/docs/report.txt"));
    let other = rig.other(cx);
    run(cx, &rig, "ctrl-enter");
    assert!(launched.lock().unwrap().is_empty());
    assert_eq!(rig.focused(cx), other);
    assert_eq!(rig.at(cx, other), here("home/docs"));
    assert_eq!(rig.cursor(cx), 1);
}

#[gpui_kit::test]
fn alt_enter_on_a_file_opens_its_folder_in_a_new_tab_on_the_file(cx: &mut TestAppContext) {
    let (rig, launched) = rig_on(cx, file("home/docs/report.txt"));
    let tabs = rig.tab_count(cx);
    run(cx, &rig, "alt-enter");
    assert!(launched.lock().unwrap().is_empty());
    assert_eq!(rig.tab_count(cx), tabs + 1);
    assert_eq!(rig.here(cx), here("home/docs"));
    assert_eq!(rig.cursor(cx), 1);
}

#[gpui_kit::test]
fn alt_enter_on_a_folder_opens_it_in_a_new_tab(cx: &mut TestAppContext) {
    let (rig, _) = rig_on(cx, Target::GoTo(location("home/docs")));
    let tabs = rig.tab_count(cx);
    run(cx, &rig, "alt-enter");
    assert_eq!(rig.tab_count(cx), tabs + 1);
    assert_eq!(rig.here(cx), here("home/docs"));
}

#[gpui_kit::test]
fn ctrl_shift_c_copies_the_full_path_of_a_file(cx: &mut TestAppContext) {
    let (rig, launched) = rig_on(cx, file("home/docs/report.txt"));
    run(cx, &rig, "ctrl-shift-c");
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied, Some(here("home/docs/report.txt")));
    assert!(launched.lock().unwrap().is_empty());
    assert_eq!(rig.here(cx), here("home"));
    assert!(!rig.is_open(cx));
    assert!(rig.pane_has_focus(cx));
}

#[gpui_kit::test]
fn ctrl_shift_c_copies_the_full_path_of_a_folder(cx: &mut TestAppContext) {
    let (rig, _) = rig_on(cx, Target::GoTo(location("home/docs")));
    run(cx, &rig, "ctrl-shift-c");
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(copied, Some(here("home/docs")));
    assert_eq!(rig.here(cx), here("home"));
}

#[gpui_kit::test]
fn enter_before_the_round_lands_launches_the_fresh_result(cx: &mut TestAppContext) {
    let (rig, launched) = rig_on(cx, file("home/a.txt"));
    rig.press(cx, "ctrl-k");
    cx.update_window(rig.window, |_, window, cx| window.input("/result", cx))
        .expect("window");
    cx.update_window(rig.window, |_, window, cx| window.press("enter", cx))
        .expect("window");
    assert!(launched.lock().unwrap().is_empty());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    settle(cx, &rig.view);
    assert_eq!(*launched.lock().unwrap(), [here("home/a.txt")]);
    assert!(!rig.is_open(cx));
}

#[gpui_kit::test]
fn a_missing_file_says_so_and_changes_nothing(cx: &mut TestAppContext) {
    let (rig, launched) = rig_on(cx, file("home/gone.txt"));
    let tabs = rig.tab_count(cx);
    for key in ["enter", "shift-enter", "alt-enter", "ctrl-enter"] {
        let before = rig.focused(cx);
        run(cx, &rig, key);
        assert_eq!(rig.focused(cx), before, "{key}");
        let notice = rig.view.read_with(cx, |view, _| view.clipboard_notice());
        assert!(notice.is_some_and(|notice| notice.is_error), "{key}");
    }
    assert!(launched.lock().unwrap().is_empty());
    assert_eq!(rig.here(cx), here("home"));
    assert_eq!(rig.tab_count(cx), tabs);
}

#[gpui_kit::test]
fn a_missing_folder_says_so_and_stays(cx: &mut TestAppContext) {
    let (rig, _) = rig_on(cx, Target::GoTo(location("home/gone")));
    run(cx, &rig, "enter");
    let notice = rig.view.read_with(cx, |view, _| view.clipboard_notice());
    assert!(notice.is_some_and(|notice| notice.is_error));
    assert_eq!(rig.here(cx), here("home"));
}

#[gpui_kit::test]
fn a_dialog_opened_during_the_look_up_keeps_the_focus(cx: &mut TestAppContext) {
    let (rig, _) = rig_on(cx, folder("home/docs"));
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "/result");
    keys_unsettled(cx, &rig, &["enter", "f7"]);
    settle(cx, &rig.view);
    cx.run_until_parked();
    assert!(prompt_open(cx, &rig.view));
    assert_eq!(rig.here(cx), here("home"));
    assert!(!rig.pane_has_focus(cx));
}
