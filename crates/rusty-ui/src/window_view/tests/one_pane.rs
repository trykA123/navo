use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, Window, px, size,
};
use rusty_core::{OpKind, PaneId};
use rusty_fs::FakeFs;

use super::drive_fs::{Setup, drive_window};
use super::{fake, home_beside_dest, open_options_sized, options_for, press};
use crate::window_view::FerriteWindow;

fn open_at(cx: &mut TestAppContext, width: f32) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    open_on(cx, fake(), width)
}

fn open_on(
    cx: &mut TestAppContext,
    fs: Arc<FakeFs>,
    width: f32,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let state = home_beside_dest(&fs);
    open_options_sized(cx, options_for(fs, state), px(width))
}

fn card_width(window: &Window, pane: PaneId) -> Option<f32> {
    window
        .try_find(("pane", pane.get()))
        .filter(|card| card.visible() && card.bounds().size.width > px(0.))
        .map(|card| f32::from(card.bounds().size.width))
}

fn shown(cx: &mut TestAppContext, window: AnyWindowHandle, panes: &[PaneId]) -> Vec<PaneId> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        panes
            .iter()
            .copied()
            .filter(|pane| card_width(window, *pane).is_some())
            .collect()
    })
    .expect("window")
}

fn resize(cx: &mut TestAppContext, window: AnyWindowHandle, width: f32) {
    VisualTestContext::from_window(window, cx).simulate_resize(size(px(width), px(600.)));
    cx.run_until_parked();
}

fn focused(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> PaneId {
    view.read_with(cx, |view, _| view.browser.state().focused_pane())
}

#[gpui_kit::test]
fn tab_switches_the_shown_pane_at_800_px(cx: &mut TestAppContext) {
    let (window, view) = open_at(cx, 800.);
    let panes = view.read_with(cx, |view, _| view.panes());
    let first = focused(cx, &view);
    assert_eq!(shown(cx, window, &panes), vec![first]);
    press(cx, window, &view, "tab");
    let second = focused(cx, &view);
    assert_ne!(first, second);
    assert_eq!(shown(cx, window, &panes), vec![second]);
    press(cx, window, &view, "tab");
    assert_eq!(focused(cx, &view), first);
    assert_eq!(shown(cx, window, &panes), vec![first]);
}

#[gpui_kit::test]
fn f5_at_800_px_targets_the_hidden_pane_folder(cx: &mut TestAppContext) {
    let fs = fake();
    let (window, view) = open_on(cx, fs.clone(), 800.);
    press(cx, window, &view, "f5");
    let pending = view.read_with(cx, |view, _| {
        view.browser
            .pending()
            .map(|p| (p.plan.kind, p.target.clone()))
    });
    assert_eq!(pending, Some((OpKind::Copy, Some(fs.location("dest")))));
}

#[gpui_kit::test]
fn widening_from_800_to_1280_px_restores_both_panes_and_the_ratio(cx: &mut TestAppContext) {
    let (window, view) = open_at(cx, 1280.);
    let panes = view.read_with(cx, |view, _| view.panes());
    view.update(cx, |view, cx| {
        view.split.update(cx, |split, _| split.set_ratio(0.3));
    });
    resize(cx, window, 800.);
    assert_eq!(shown(cx, window, &panes).len(), 1);
    press(cx, window, &view, "tab");
    resize(cx, window, 1280.);
    assert_eq!(shown(cx, window, &panes), panes);
    let ratio = view.read_with(cx, |view, cx| view.split.read(cx).ratio());
    assert!((ratio - 0.3).abs() < f32::EPSILON);
    let (left, right) = cx
        .update_window(window, |_, window, _| {
            (card_width(window, panes[0]), card_width(window, panes[1]))
        })
        .expect("window");
    let (left, right) = (left.expect("left"), right.expect("right"));
    assert!(left < right * 0.6, "left {left} px, right {right} px");
}

#[gpui_kit::test]
fn alt_f1_at_800_px_shows_the_hidden_pane_while_its_drive_menu_is_open(cx: &mut TestAppContext) {
    let rig = drive_window(cx, Setup::default());
    let panes = rig.panes(cx);
    let start = rig.focused(cx);
    assert_eq!(start, panes[1]);
    resize(cx, rig.window, 800.);
    assert_eq!(shown(cx, rig.window, &panes), vec![start]);
    press(cx, rig.window, &rig.view, "alt-f1");
    assert_eq!(shown(cx, rig.window, &panes), vec![panes[0]]);
    rig.frame(cx, |window, _| {
        assert!(window.try_find(("drive-menu", panes[0].get())).is_some());
    });
    press(cx, rig.window, &rig.view, "escape");
    assert_eq!(rig.focused(cx), start);
    assert_eq!(shown(cx, rig.window, &panes), vec![start]);
    press(cx, rig.window, &rig.view, "tab");
    assert_eq!(shown(cx, rig.window, &panes), vec![panes[0]]);
}
