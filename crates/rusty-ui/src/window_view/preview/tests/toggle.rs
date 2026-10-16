use gpui_kit::AppContext as _;
use gpui_kit::TestAppContext;
use gpui_kit::test::TestWindowExt;

use super::{Rig, open_fake};
use crate::window_view::tests::fake;

fn is_open(cx: &mut TestAppContext, rig: &Rig) -> bool {
    rig.read(cx, |view| view.preview.is_open())
}

fn button_pressed(cx: &mut TestAppContext, rig: &Rig) -> Option<bool> {
    let pane = rig.active_pane(cx);
    cx.update_window(rig.window, |_, window, cx| {
        window.render_frame(cx);
        window.find(("preview-button", pane)).selected()
    })
    .expect("window")
}

#[gpui_kit::test]
fn ctrl_q_opens_and_closes_the_preview_and_the_list_keeps_focus(cx: &mut TestAppContext) {
    let rig = open_fake(cx, &fake());
    assert!(!is_open(cx, &rig));
    assert!(!rig.shows(cx, "preview-sheet"));
    rig.key(cx, "ctrl-q");
    assert!(is_open(cx, &rig));
    rig.settle_motion(cx);
    assert!(rig.shows(cx, "preview-sheet"));
    assert!(rig.list_has_focus(cx));
    rig.key(cx, "ctrl-q");
    assert!(!is_open(cx, &rig));
    assert!(rig.list_has_focus(cx));
}

#[gpui_kit::test]
fn the_preview_button_in_the_active_card_toggles_the_sheet(cx: &mut TestAppContext) {
    let rig = open_fake(cx, &fake());
    let pane = rig.active_pane(cx);
    assert_eq!(button_pressed(cx, &rig), Some(false));
    rig.click(cx, ("preview-button", pane));
    assert!(is_open(cx, &rig));
    assert_eq!(button_pressed(cx, &rig), Some(true));
    assert!(rig.list_has_focus(cx));
    rig.click(cx, ("preview-button", pane));
    assert!(!is_open(cx, &rig));
}

#[gpui_kit::test]
fn only_the_active_card_has_a_preview_button(cx: &mut TestAppContext) {
    let rig = open_fake(cx, &fake());
    let active = rig.active_pane(cx);
    let panes = rig.read(cx, |view| view.panes());
    let passive = panes
        .iter()
        .map(|pane| pane.get())
        .find(|pane| *pane != active)
        .expect("two panes");
    assert!(rig.shows(cx, ("preview-button", active)));
    assert!(!rig.shows(cx, ("preview-button", passive)));
}

#[gpui_kit::test]
fn the_open_state_outlives_the_viewer(cx: &mut TestAppContext) {
    let rig = open_fake(cx, &fake());
    rig.open(cx);
    rig.key(cx, "down");
    rig.key(cx, "f3");
    assert!(rig.read(cx, |view| view.viewer.is_some()));
    assert!(!rig.shows(cx, "preview-sheet"));
    rig.key(cx, "escape");
    assert!(rig.read(cx, |view| view.viewer.is_none()));
    assert!(is_open(cx, &rig));
    rig.settle_motion(cx);
    assert!(rig.shows(cx, "preview-sheet"));
}

#[gpui_kit::test]
fn the_footer_names_every_preview_key(cx: &mut TestAppContext) {
    let rig = open_fake(cx, &fake());
    rig.open(cx);
    let hints: Vec<String> = (0..5usize)
        .filter_map(|index| rig.label(cx, ("side-sheet-hint", index)))
        .collect();
    assert_eq!(hints, ["Browse", "Open", "Stop scan", "[ ]", "Close"]);
}
