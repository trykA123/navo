//! Tests of the `Sidebar` rail tooltips: hover, focus, Escape latch and tab-change drops.

use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Modifiers, Point, Role, TestAppContext, VisualTestContext, point, px,
};
use navo_compare_profile::appearance::PaletteId;
use sidewise_kit::tokens::Mode;

use super::sidebar_tests::{HostRig, open_host_with_outside};
use super::*;
use crate::theme::apply_sidewise;

/// Opens the host in rail mode with the first frames settled; returns the rig.
fn rail_rig(cx: &mut TestAppContext) -> HostRig {
    let rig = open_host_with_outside(cx);
    rig.view.update(cx, |sb, cx| sb.toggle_rail(cx));
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    rig
}

/// Moves the pointer to `pt` with no modifiers.
fn move_mouse(cx: &mut TestAppContext, window: AnyWindowHandle, pt: Point<Pixels>) {
    VisualTestContext::from_window(window, cx).simulate_mouse_move(pt, None, Modifiers::none());
}

/// The centre of the control `id`.
fn center_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: impl Into<ElementId> + 'static,
) -> Point<Pixels> {
    cx.update_window(window, |_, window, _| window.find(id).bounds().center())
        .expect("window")
}

/// Renders bounded frames until `"rail-tooltip"` presence matches `want` (no fixed sleep).
/// Panics with the wanted state and the tip label if it never settles.
#[track_caller]
fn settle_tip(cx: &mut TestAppContext, window: AnyWindowHandle, want: bool) {
    for _ in 0..6 {
        cx.run_until_parked();
        let matched = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window.try_find("rail-tooltip").is_some() == want
            })
            .expect("window");
        if matched {
            return;
        }
    }
    let label = tip_label(cx, window);
    panic!("tooltip did not settle: wanted shown={want}, tip label={label:?}");
}

/// The visible tooltip's label, if any.
fn tip_label(cx: &mut TestAppContext, window: AnyWindowHandle) -> Option<String> {
    cx.update_window(window, |_, window, _| {
        window
            .try_find("rail-tooltip")
            .and_then(|t| t.label().map(String::from))
    })
    .expect("window")
}

/// Rail hover shows the tip 10 px right of the outer row for both ring widths; static rows stay silent.
#[gpui_kit::test]
fn rail_hover_shows_the_tip_beside_the_outer_row_and_static_rows_stay_silent(
    cx: &mut TestAppContext,
) {
    let rig = rail_rig(cx);
    for (palette, mode, ring) in [
        (PaletteId::Violet, Mode::Light, 2.0_f32),
        (PaletteId::Contrast, Mode::Dark, 3.0_f32),
    ] {
        cx.update(|cx| apply_sidewise(palette, mode, cx));
        assert_eq!(cx.update(|cx| SidewiseTheme::global(cx).spec.ring_px), ring);
        let rows: [(ElementId, &str); 2] = [
            ("new-compare".into(), "New comparison"),
            (("open-tab", 0usize).into(), "tab"),
        ];
        for (id, label) in rows {
            let pt = center_of(cx, rig.window, id.clone());
            move_mouse(cx, rig.window, pt);
            settle_tip(cx, rig.window, true);
            cx.update_window(rig.window, |_, window, _| {
                let row = window.find(id.clone()).bounds();
                let tip = window.find("rail-tooltip");
                assert_eq!(tip.label(), Some(label));
                assert_eq!(tip.role(), Some(Role::Tooltip));
                assert_eq!(
                    tip.bounds().origin.x,
                    row.right() + px(10.),
                    "{id:?} {ring}px"
                );
                assert_eq!(tip.bounds().origin.y, row.origin.y);
            })
            .expect("window");
        }
        let recent = center_of(cx, rig.window, ("Recent", 0usize));
        move_mouse(cx, rig.window, recent);
        settle_tip(cx, rig.window, false);
    }
}

/// Keyboard focus shows the tip, row hover beats it, and leaving the row falls back to focus.
#[gpui_kit::test]
fn focus_shows_the_tip_and_row_hover_beats_it(cx: &mut TestAppContext) {
    let rig = rail_rig(cx);
    cx.update_window(rig.window, |_, window, cx| {
        rig.view.update(cx, |sb, cx| sb.focus_primary(window, cx));
    })
    .expect("window");
    settle_tip(cx, rig.window, true);
    assert_eq!(tip_label(cx, rig.window).as_deref(), Some("New comparison"));

    let tab = center_of(cx, rig.window, ("open-tab", 0usize));
    move_mouse(cx, rig.window, tab);
    settle_tip(cx, rig.window, true);
    assert_eq!(tip_label(cx, rig.window).as_deref(), Some("tab"));

    move_mouse(cx, rig.window, point(px(700.), px(500.)));
    settle_tip(cx, rig.window, true);
    assert_eq!(tip_label(cx, rig.window).as_deref(), Some("New comparison"));
}

/// Escape consumes while a tip shows, propagates once it is gone, and stays latched under a resting
/// pointer and focus until every source for that row leaves.
#[gpui_kit::test]
fn escape_dismisses_the_tip_and_latches_until_all_sources_leave(cx: &mut TestAppContext) {
    let rig = rail_rig(cx);
    assert!(
        !rig.view.update(cx, |sb, cx| sb.dismiss_tip(cx)),
        "no tip: nothing to dismiss"
    );
    cx.update_window(rig.window, |_, window, cx| {
        rig.view.update(cx, |sb, cx| sb.focus_primary(window, cx));
    })
    .expect("window");
    let row = center_of(cx, rig.window, "new-compare");
    move_mouse(cx, rig.window, row);
    settle_tip(cx, rig.window, true);

    let before = rig.parent_keys.get();
    cx.simulate_keystrokes(rig.window, "escape");
    settle_tip(cx, rig.window, false);
    assert_eq!(
        rig.parent_keys.get(),
        before,
        "a dismissing Escape is consumed"
    );
    cx.simulate_keystrokes(rig.window, "escape");
    assert_eq!(
        rig.parent_keys.get(),
        before + 1,
        "an idle Escape propagates"
    );
    move_mouse(cx, rig.window, point(px(700.), px(500.)));
    settle_tip(cx, rig.window, false);
    move_mouse(cx, rig.window, row);
    settle_tip(cx, rig.window, false);

    move_mouse(cx, rig.window, point(px(700.), px(500.)));
    cx.update_window(rig.window, |_, window, cx| rig.outside.focus(window, cx))
        .expect("window");
    move_mouse(cx, rig.window, row);
    settle_tip(cx, rig.window, true);
}

/// Escape while the pointer is on the tip itself removes it and keeps it gone under the resting pointer.
#[gpui_kit::test]
fn escape_on_the_hovered_overlay_stays_dismissed_under_a_resting_pointer(cx: &mut TestAppContext) {
    let rig = rail_rig(cx);
    let row = center_of(cx, rig.window, ("open-tab", 0usize));
    move_mouse(cx, rig.window, row);
    settle_tip(cx, rig.window, true);
    let tip = center_of(cx, rig.window, "rail-tooltip");
    move_mouse(cx, rig.window, tip);
    settle_tip(cx, rig.window, true);
    assert_eq!(tip_label(cx, rig.window).as_deref(), Some("tab"));

    let before = rig.parent_keys.get();
    cx.simulate_keystrokes(rig.window, "escape");
    settle_tip(cx, rig.window, false);
    assert_eq!(
        rig.parent_keys.get(),
        before,
        "the dismissing Escape is consumed"
    );
    move_mouse(cx, rig.window, tip);
    for _ in 0..3 {
        settle_tip(cx, rig.window, false);
    }
    cx.update_window(rig.window, |_, window, _| {
        assert!(
            window.try_find("rail-tooltip-bridge").is_none(),
            "no ghost bridge"
        );
    })
    .expect("window");

    move_mouse(cx, rig.window, point(px(700.), px(500.)));
    move_mouse(cx, rig.window, row);
    settle_tip(cx, rig.window, true);
}

/// Expanding the rail, relabelling a tab and removing a tab each drop a tip that was showing.
#[gpui_kit::test]
fn expansion_and_tab_changes_drop_the_tip(cx: &mut TestAppContext) {
    let rig = rail_rig(cx);
    let tab = center_of(cx, rig.window, ("open-tab", 0usize));
    move_mouse(cx, rig.window, tab);
    settle_tip(cx, rig.window, true);
    rig.view
        .update(cx, |sb, cx| sb.set_tabs(vec!["renamed".into()], 0, cx));
    settle_tip(cx, rig.window, false);

    rig.view.update(cx, |sb, cx| {
        sb.set_tabs(vec!["a".into(), "b".into()], 0, cx)
    });
    cx.update_window(rig.window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    let second = center_of(cx, rig.window, ("open-tab", 1usize));
    move_mouse(cx, rig.window, second);
    settle_tip(cx, rig.window, true);
    rig.view
        .update(cx, |sb, cx| sb.set_tabs(vec!["a".into()], 0, cx));
    settle_tip(cx, rig.window, false);

    let first = center_of(cx, rig.window, "new-compare");
    move_mouse(cx, rig.window, first);
    settle_tip(cx, rig.window, true);
    rig.view.update(cx, |sb, cx| sb.toggle_rail(cx));
    settle_tip(cx, rig.window, false);
    let expanded = center_of(cx, rig.window, "new-compare");
    move_mouse(cx, rig.window, expanded);
    settle_tip(cx, rig.window, false);
}

/// An identical `set_tabs` refresh keeps a tip the pointer rests on.
#[gpui_kit::test]
fn identical_tabs_refresh_keeps_a_resting_tip(cx: &mut TestAppContext) {
    let rig = rail_rig(cx);
    let tab = center_of(cx, rig.window, ("open-tab", 0usize));
    move_mouse(cx, rig.window, tab);
    settle_tip(cx, rig.window, true);
    let tip = center_of(cx, rig.window, "rail-tooltip");
    move_mouse(cx, rig.window, tip);
    settle_tip(cx, rig.window, true);
    rig.view
        .update(cx, |sb, cx| sb.set_tabs(vec!["tab".into()], 0, cx));
    settle_tip(cx, rig.window, true);
    assert_eq!(tip_label(cx, rig.window).as_deref(), Some("tab"));
}
