use std::sync::Arc;

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, TestAppContext, VisualTestContext, Window, px, size,
};
use rusty_core::{Command, OpKind, PaneId, Tiles, apply};
use rusty_fs::FakeFs;

use super::drive_fs::{Setup, drive_window};
use super::{fake, home_beside_dest, open_options_sized, options_for, press};
use crate::tokens::{self, Mode};
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
        view.browser
            .command(&rusty_core::Command::LayoutRatio {
                split: rusty_core::SplitPath(Vec::new()),
                ratio: 300,
            })
            .expect("ratio");
        cx.notify();
    });
    resize(cx, window, 800.);
    assert_eq!(shown(cx, window, &panes).len(), 1);
    press(cx, window, &view, "tab");
    resize(cx, window, 1280.);
    assert_eq!(shown(cx, window, &panes), panes);
    let ratio = view.read_with(cx, |view, _| view.root_ratio());
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

fn open_preset(
    cx: &mut TestAppContext,
    tiles: Tiles,
    width: f32,
) -> (AnyWindowHandle, Entity<FerriteWindow>) {
    let fs = fake();
    let mut state = home_beside_dest(&fs);
    apply(&mut state, &Command::LayoutPreset { tiles }).expect("preset");
    let opened = open_options_sized(cx, options_for(fs, state), px(width));
    VisualTestContext::from_window(opened.0, cx).simulate_resize(size(px(width), px(800.)));
    cx.run_until_parked();
    opened
}

fn strip_buttons(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<usize> {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.render_frame(cx);
        window.try_find("pane-strip").map(|_| {
            (1..=4usize)
                .filter(|n| window.try_find(("pane-strip-button", *n)).is_some())
                .count()
        })
    })
    .expect("window")
}

fn number_of(view: &FerriteWindow, pane: Option<PaneId>) -> usize {
    let numbered = view.core_layout().expect("layout").numbered();
    numbered
        .iter()
        .position(|id| Some(*id) == pane)
        .expect("numbered pane")
        + 1
}

fn two_by_two() -> Tiles {
    Tiles::builtin()[3].tiles.clone()
}

#[gpui_kit::test]
fn four_panes_at_800_px_show_a_strip_with_four_buttons(cx: &mut TestAppContext) {
    let (window, _) = open_preset(cx, two_by_two(), 800.);
    assert_eq!(strip_buttons(cx, window), Some(4));
}

#[gpui_kit::test]
fn a_pane_under_280_px_at_1280_px_shows_the_strip(cx: &mut TestAppContext) {
    let tiles = Tiles::parse("row(15, pane, pane)").expect("tiles");
    let (window, _) = open_preset(cx, tiles, 1280.);
    assert_eq!(strip_buttons(cx, window), Some(2));
}

#[gpui_kit::test]
fn two_panes_at_1280_px_show_no_strip(cx: &mut TestAppContext) {
    let (window, _) = open_at(cx, 1280.);
    assert_eq!(strip_buttons(cx, window), None);
}

#[gpui_kit::test]
fn clicking_button_3_focuses_pane_3_and_moves_the_tags(cx: &mut TestAppContext) {
    let (window, view) = open_preset(cx, two_by_two(), 800.);
    let before = view.read_with(cx, |view, _| {
        number_of(view, Some(view.browser.state().focused_pane()))
    });
    assert_ne!(before, 3);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click(("pane-strip-button", 3usize), cx);
    })
    .expect("window");
    cx.run_until_parked();
    let (focused, target) = view.read_with(cx, |view, _| {
        let state = view.browser.state();
        (
            number_of(view, Some(state.focused_pane())),
            number_of(view, view.browser.target_pane()),
        )
    });
    assert_eq!(focused, 3);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("pane-strip-active", 3usize)).is_some());
        assert!(window.try_find(("pane-strip-active", before)).is_none());
        assert!(window.try_find(("pane-strip-target", target)).is_some());
    })
    .expect("window");
}

#[gpui_kit::test]
fn strip_number_name_and_tag_share_one_centre_line(cx: &mut TestAppContext) {
    let (window, view) = open_preset(cx, two_by_two(), 800.);
    let (active, target) = view.read_with(cx, |view, _| {
        (
            number_of(view, Some(view.browser.state().focused_pane())),
            number_of(view, view.browser.target_pane()),
        )
    });
    for mode in [Mode::Light, Mode::Dark] {
        cx.update(|cx| tokens::apply(mode, cx));
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            for (n, tag) in [(active, "pane-strip-active"), (target, "pane-strip-target")] {
                let centre = |id: &'static str| window.find((id, n)).bounds().center().y;
                let number = centre("pane-strip-number");
                let button = window.find(("pane-strip-button", n)).bounds().center().y;
                for other in [centre("pane-strip-name"), centre(tag), button] {
                    assert!((number - other).abs() <= px(1.), "{number:?} vs {other:?}");
                }
            }
        })
        .expect("window");
    }
}

fn focused_number(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> usize {
    view.read_with(cx, |view, _| {
        number_of(view, Some(view.browser.state().focused_pane()))
    })
}

#[gpui_kit::test]
fn ctrl_shift_3_focuses_pane_3_wide_and_in_one_pane_mode(cx: &mut TestAppContext) {
    for width in [1280., 800.] {
        let (window, view) = open_preset(cx, Tiles::builtin()[1].tiles.clone(), width);
        let panes = view.read_with(cx, |view, _| view.tiles_layout().numbered());
        assert_eq!(panes.len(), 3);
        assert_ne!(focused_number(cx, &view), 3);
        press(cx, window, &view, "ctrl-shift-3");
        assert_eq!(focused_number(cx, &view), 3);
        press(cx, window, &view, "ctrl-shift-4");
        assert_eq!(focused_number(cx, &view), 3);
        if width < 900. {
            assert_eq!(shown(cx, window, &panes), vec![panes[2]]);
        }
    }
}

#[gpui_kit::test]
fn a_split_in_one_pane_mode_draws_no_fade_cover(cx: &mut TestAppContext) {
    let (window, view) = open_at(cx, 800.);
    press(cx, window, &view, "ctrl-l");
    press(cx, window, &view, "v");
    assert_eq!(view.read_with(cx, |view, _| view.panes().len()), 3);
    assert_eq!(view.read_with(cx, |view, _| view.tiles.fades.count()), 0);
}
