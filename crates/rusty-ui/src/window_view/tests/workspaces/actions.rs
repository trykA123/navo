use std::rc::Rc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, Modifiers, TestAppContext};

use super::super::fkeys::hold;
use super::super::{fake, open_with, press};
use super::{rig, shape};
use crate::window_view::FerriteWindow;

fn show_tray(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<FerriteWindow>) {
    view.update(cx, |view, cx| {
        view.workspace_keys.visible = true;
        cx.notify();
    });
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
}

fn click(cx: &mut TestAppContext, window: AnyWindowHandle, card: usize, id: &'static str) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within(("workspace-card", card))
            .click((id, card), cx);
        window.render_frame(cx);
    })
    .expect("window");
    cx.run_until_parked();
}

fn names(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Vec<String> {
    view.read_with(cx, |view, _| {
        let state = view.browser.state();
        let core = state.window(state.focused_window()).expect("core window");
        core.workspaces().iter().map(|w| w.name.clone()).collect()
    })
}

fn has(cx: &mut TestAppContext, window: AnyWindowHandle, card: usize, id: &'static str) -> bool {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .within(("workspace-card", card))
            .try_find((id, card))
            .is_some()
    })
    .expect("window")
}

fn tabs(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> usize {
    view.read_with(cx, |view, _| {
        let state = view.browser.state();
        state.pane(state.focused_pane()).expect("pane").tabs().len()
    })
}

fn rename_second_by_f2(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let (window, view) = rig(cx);
    show_tray(cx, window, &view);
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| {
            window.focus(&view.workspace_keys.edit.cards[1], cx);
        });
    })
    .expect("window");
    press(cx, window, &view, "f2");
    assert!(has(cx, window, 2, "workspace-rename-field"));
    cx.update_window(window, |_, window, cx| window.input("Reports", cx))
        .expect("window");
    (window, view)
}

#[gpui_kit::test]
fn f2_on_a_focused_card_renames_it_and_enter_keeps_the_name(cx: &mut TestAppContext) {
    let (window, view) = rename_second_by_f2(cx);
    press(cx, window, &view, "enter");
    assert_eq!(names(cx, &view)[1], "Reports");
    assert!(!has(cx, window, 2, "workspace-rename-field"));
    assert!(view.read_with(cx, |view, _| view.workspace_keys.visible));
}

#[gpui_kit::test]
fn escape_cancels_a_rename(cx: &mut TestAppContext) {
    let (window, view) = rename_second_by_f2(cx);
    let before = names(cx, &view);
    press(cx, window, &view, "escape");
    assert_eq!(names(cx, &view), before);
    assert!(!has(cx, window, 2, "workspace-rename-field"));
}

#[gpui_kit::test]
fn the_pencil_opens_the_rename_field(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    show_tray(cx, window, &view);
    click(cx, window, 1, "workspace-rename");
    assert!(has(cx, window, 1, "workspace-rename-field"));
}

#[gpui_kit::test]
fn move_right_changes_the_number(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    show_tray(cx, window, &view);
    let before = names(cx, &view);
    click(cx, window, 1, "workspace-move-right");
    assert_eq!(names(cx, &view), vec![before[1].clone(), before[0].clone()]);
    assert_eq!(shape(cx, &view), (1, 2));
}

#[gpui_kit::test]
fn delete_then_ctrl_z_restores_at_the_same_number(cx: &mut TestAppContext) {
    let (window, view) = super::three_workspaces(cx);
    hold(cx, window, Modifiers::default());
    show_tray(cx, window, &view);
    let before = names(cx, &view);
    click(cx, window, 2, "workspace-delete");
    assert_eq!(names(cx, &view), vec![before[0].clone(), before[2].clone()]);
    let notice = view.read_with(cx, |view, _| {
        view.status_notice().map(|notice| notice.text.to_string())
    });
    assert!(notice.is_some_and(|text| text.contains(&before[1])));
    press(cx, window, &view, "ctrl-z");
    assert_eq!(names(cx, &view), before);
}

#[gpui_kit::test]
fn delete_is_disabled_for_the_last_workspace(cx: &mut TestAppContext) {
    let fs = fake();
    let mut state = super::super::two_panes(&fs);
    rusty_core::apply(&mut state, &rusty_core::Command::WorkspaceSave).expect("save");
    let (window, view) = open_with(cx, fs, state, Rc::default());
    hold(cx, window, Modifiers::default());
    show_tray(cx, window, &view);
    click(cx, window, 1, "workspace-delete");
    assert_eq!(names(cx, &view).len(), 1);
}

#[gpui_kit::test]
fn save_clears_the_draft(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    press(cx, window, &view, "ctrl-t");
    show_tray(cx, window, &view);
    assert!(has(cx, window, 1, "workspace-draft"));
    assert!(!has(cx, window, 1, "workspace-rename"));
    click(cx, window, 1, "workspace-save");
    assert!(!has(cx, window, 1, "workspace-draft"));
    assert!(has(cx, window, 1, "workspace-rename"));
}

#[gpui_kit::test]
fn discard_restores_the_saved_tabs(cx: &mut TestAppContext) {
    let (window, view) = rig(cx);
    let saved = tabs(cx, &view);
    press(cx, window, &view, "ctrl-t");
    assert_eq!(tabs(cx, &view), saved + 1);
    show_tray(cx, window, &view);
    click(cx, window, 1, "workspace-discard");
    assert_eq!(tabs(cx, &view), saved);
    assert!(!has(cx, window, 1, "workspace-draft"));
}
