use std::time::Duration;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

use super::super::{
    home_beside_dest, open_options, options_for, press, prompt_open, settle, viewer_open,
};
use super::{Rig, fake, hit, location, recording, rig, rig_with};
use crate::command_center::{Group, Notes, Scope, Target};

pub(super) fn keys_unsettled(cx: &mut TestAppContext, rig: &Rig, keys: &[&str]) {
    cx.update_window(rig.window, |_, window, cx| {
        for key in keys {
            window.press(key, cx);
        }
    })
    .expect("window");
}

pub(super) fn input_unsettled(cx: &mut TestAppContext, rig: &Rig, text: &str) {
    cx.update_window(rig.window, |_, window, cx| window.input(text, cx))
        .expect("window");
}

fn open_with_marked_folder(cx: &mut TestAppContext) -> Rig {
    let fs = fake();
    let state = home_beside_dest(&fs);
    let (window, view) = open_options(cx, options_for(fs, state));
    press(cx, window, &view, "space");
    Rig { window, view }
}

#[gpui_kit::test]
fn enter_before_the_new_round_lands_runs_the_new_text(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let seen = recording(cx, &rig);
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "refresh");
    assert!(rig.rows(cx).iter().any(|row| row.starts_with("Refresh")));
    keys_unsettled(cx, &rig, &["ctrl-a"]);
    input_unsettled(cx, &rig, "switch pane");
    keys_unsettled(cx, &rig, &["enter"]);
    assert!(seen.lock().unwrap().is_empty());
    settle(cx, &rig.view);
    cx.run_until_parked();
    assert_eq!(
        *seen.lock().unwrap(),
        vec!["file_list::SwitchPane".to_owned()]
    );
}

#[gpui_kit::test]
fn typing_again_cancels_a_waiting_enter(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let seen = recording(cx, &rig);
    rig.press(cx, "ctrl-k");
    input_unsettled(cx, &rig, "switch pane");
    keys_unsettled(cx, &rig, &["enter"]);
    input_unsettled(cx, &rig, "x");
    settle(cx, &rig.view);
    cx.run_until_parked();
    assert!(seen.lock().unwrap().is_empty());
    assert!(rig.is_open(cx));
}

#[gpui_kit::test]
fn commands_show_before_the_file_sources_answer(cx: &mut TestAppContext) {
    let folder = Target::GoTo(location("dest"));
    let rig = rig_with(cx, vec![hit("switch-folder", Group::Folders, folder)]);
    rig.press(cx, "ctrl-k");
    input_unsettled(cx, &rig, "switch");
    cx.run_until_parked();
    let early = rig.rows(cx);
    assert!(
        early.iter().any(|row| row.starts_with("Switch pane")),
        "{early:?}"
    );
    assert!(
        !early.iter().any(|row| row.starts_with("switch-folder")),
        "{early:?}"
    );
    cx.executor().advance_clock(Duration::from_millis(100));
    settle(cx, &rig.view);
    let late = rig.rows(cx);
    assert!(
        late.iter().any(|row| row.starts_with("Switch pane")),
        "{late:?}"
    );
    assert!(
        late.iter().any(|row| row.starts_with("switch-folder")),
        "{late:?}"
    );
}

#[gpui_kit::test]
fn hits_of_a_closed_box_never_show_in_the_next_one(cx: &mut TestAppContext) {
    let rig = rig(cx);
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "switch pane");
    let old = rig
        .view
        .read_with(cx, |view, _| view.command_center_generation())
        .expect("open");
    rig.press(cx, "ctrl-k");
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "switch pane");
    let shown = rig.rows(cx);
    let stale = hit(
        "Stale",
        Group::Commands,
        Target::Action("file_list::Refresh"),
    );
    cx.update_window(rig.window, |_, window, cx| {
        rig.view.update(cx, |view, cx| {
            let hits = (old, Scope::Mixed, vec![stale], Notes::default());
            view.receive_hits(hits, window, cx);
        });
    })
    .expect("window");
    assert_eq!(rig.rows(cx), shown);
}

#[gpui_kit::test]
fn an_unregistered_action_records_nothing_and_says_so(cx: &mut TestAppContext) {
    let missing = Target::Action("gone::Missing");
    let rig = rig_with(cx, vec![hit("Vanished", Group::Commands, missing)]);
    let seen = recording(cx, &rig);
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "vanished");
    rig.press(cx, "enter");
    cx.run_until_parked();
    assert!(seen.lock().unwrap().is_empty());
    assert!(!rig.is_open(cx));
    let notice = rig.view.read_with(cx, |view, _| view.clipboard_notice());
    assert!(notice.is_some_and(|notice| notice.is_error));
}

#[gpui_kit::test]
fn delete_from_the_box_asks_before_it_deletes(cx: &mut TestAppContext) {
    let rig = open_with_marked_folder(cx);
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "delete permanently");
    rig.press(cx, "enter");
    assert!(!rig.is_open(cx));
    assert!(
        rig.view
            .read_with(cx, |view, _| view.browser.pending().is_some())
    );
}

#[gpui_kit::test]
fn the_box_does_not_open_over_a_confirm_or_a_prompt(cx: &mut TestAppContext) {
    let rig = open_with_marked_folder(cx);
    rig.press(cx, "f8");
    assert!(
        rig.view
            .read_with(cx, |view, _| view.browser.pending().is_some())
    );
    rig.press(cx, "ctrl-k");
    assert!(!rig.is_open(cx));
    rig.press(cx, "escape");
    rig.press(cx, "f7");
    assert!(prompt_open(cx, &rig.view));
    rig.press(cx, "ctrl-k");
    assert!(!rig.is_open(cx));
}

#[gpui_kit::test]
fn commands_of_the_viewer_show_when_it_had_focus(cx: &mut TestAppContext) {
    let rig = rig(cx);
    let file = location("home/a.txt");
    cx.update_window(rig.window, |_, window, cx| {
        rig.view
            .update(cx, |view, cx| view.view_location(file, window, cx));
    })
    .expect("window");
    settle(cx, &rig.view);
    assert!(viewer_open(cx, &rig.view));
    rig.press(cx, "ctrl-k");
    rig.type_query(cx, "close viewer");
    let rows = rig.rows(cx);
    assert!(
        rows.iter().any(|row| row.starts_with("Close viewer")),
        "{rows:?}"
    );
}
