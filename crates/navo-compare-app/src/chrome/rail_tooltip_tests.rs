//! Unit and overlay window tests for collapsed-rail tooltips (`A1a`, `A1b`, and `A2`).

use gpui_kit::base::test_support::{find, texts};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, Bounds, ElementId, FocusHandle, HoverListenerMode, KeyBinding, Modifiers,
    Point, ReadGlobal as _, Render, Role, TestAppContext, VisualTestContext, WindowBounds,
    WindowOptions, canvas, div, point, px, size,
};
use navo_compare_profile::appearance::PaletteId;
use sidewise_kit::tokens::Mode;

use super::*;
use crate::theme::{SidewiseTheme, apply_sidewise};

#[gpui_kit::test]
fn priority_prefers_row_over_overlay_over_focus_and_falls_back_in_order() {
    let mut tips = RailTips::new();
    let new_cmp = Target::new(RowKey::NewCompare, "New comparison");
    let tab0 = Target::new(RowKey::Tab(0), "Alpha vs Beta");
    let tab0_row = Target::new(RowKey::Tab(0), "Alpha vs Beta (row)");

    assert_eq!(tips.active_target(), None);

    tips.sync_focus(Some(new_cmp.clone()));
    assert_eq!(tips.active_target(), Some(&new_cmp));

    assert!(tips.set_hovered(HoverPlace::Overlay, tab0.clone(), true));
    assert_eq!(tips.active_target(), Some(&tab0));

    assert!(tips.set_hovered(HoverPlace::Row, tab0_row.clone(), true));
    assert_eq!(tips.active_target(), Some(&tab0_row));
    assert!(!tips.set_hovered(HoverPlace::Row, tab0_row.clone(), true));

    assert!(tips.set_hovered(HoverPlace::Row, tab0_row, false));
    assert_eq!(tips.active_target(), Some(&tab0));

    assert!(tips.set_hovered(HoverPlace::Overlay, tab0, false));
    assert_eq!(tips.active_target(), Some(&new_cmp));

    tips.sync_focus(None);
    assert_eq!(tips.active_target(), None);
}

#[gpui_kit::test]
fn out_of_order_hover_exits_clear_only_matching_place_and_key() {
    let mut tips = RailTips::new();
    let tab0 = Target::new(RowKey::Tab(0), "First");
    let tab1 = Target::new(RowKey::Tab(1), "Second");

    assert!(tips.set_hovered(HoverPlace::Row, tab0.clone(), true));
    assert!(tips.set_hovered(HoverPlace::Row, tab1.clone(), true));
    assert!(!tips.set_hovered(HoverPlace::Row, tab0.clone(), false));
    assert_eq!(tips.active_target(), Some(&tab1));

    assert!(!tips.set_hovered(HoverPlace::Overlay, tab1.clone(), true));
    assert!(!tips.set_hovered(HoverPlace::Overlay, tab0, false));
    assert!(!tips.set_hovered(HoverPlace::Row, tab1.clone(), false));
    assert_eq!(tips.active_target(), Some(&tab1));

    assert!(tips.set_hovered(HoverPlace::Overlay, tab1, false));
    assert_eq!(tips.active_target(), None);
}

#[gpui_kit::test]
fn entering_different_row_clears_stale_overlay_target() {
    let mut tips = RailTips::new();
    let focus_row = Target::new(RowKey::NewCompare, "New comparison");
    let tab0 = Target::new(RowKey::Tab(0), "Alpha");
    let tab1 = Target::new(RowKey::Tab(1), "Beta");

    tips.sync_focus(Some(focus_row.clone()));
    assert!(tips.set_hovered(HoverPlace::Overlay, tab0.clone(), true));
    assert_eq!(tips.active_target(), Some(&tab0));

    assert!(tips.set_hovered(HoverPlace::Row, tab1.clone(), true));
    assert_eq!(tips.active_target(), Some(&tab1));

    assert!(tips.set_hovered(HoverPlace::Row, tab1, false));
    assert_eq!(
        tips.active_target(),
        Some(&focus_row),
        "old overlay hover must not resurrect after leaving a different row"
    );
}

#[gpui_kit::test]
fn sync_focus_replaces_label_and_clear_resets_all_targets() {
    let mut tips = RailTips::new();
    let tab0_old = Target::new(RowKey::Tab(0), "Old tab title");
    let tab0_new = Target::new(RowKey::Tab(0), "Updated tab title");
    let tab1 = Target::new(RowKey::Tab(1), "Second");

    tips.sync_focus(Some(tab0_old));
    assert!(tips.set_hovered(HoverPlace::Overlay, tab1.clone(), true));
    assert!(!tips.set_hovered(HoverPlace::Row, tab1.clone(), true));

    tips.sync_focus(Some(tab0_new.clone()));
    assert_eq!(tips.active_target(), Some(&tab1));

    assert!(!tips.set_hovered(HoverPlace::Row, tab1.clone(), false));
    assert!(tips.set_hovered(HoverPlace::Overlay, tab1.clone(), false));
    assert_eq!(tips.active_target(), Some(&tab0_new));

    assert!(tips.set_hovered(HoverPlace::Row, tab1.clone(), true));
    assert!(!tips.set_hovered(HoverPlace::Overlay, tab1, true));
    tips.clear();
    assert_eq!(tips.active_target(), None);

    tips.sync_focus(Some(tab0_new.clone()));
    assert_eq!(tips.active_target(), Some(&tab0_new));
}

#[gpui_kit::test]
fn dismiss_clears_only_after_both_hover_and_focus_leave_target() {
    let mut tips = RailTips::new();
    let tab0 = Target::new(RowKey::Tab(0), "Alpha");
    let tab1 = Target::new(RowKey::Tab(1), "Beta");
    let tab2 = Target::new(RowKey::Tab(2), "Gamma");

    // Exit order 1: hover exits before focus blurs; re-hover stays suppressed until focus leaves.
    tips.sync_focus(Some(tab0.clone()));
    assert!(!tips.set_hovered(HoverPlace::Overlay, tab0.clone(), true));
    assert!(!tips.set_hovered(HoverPlace::Row, tab0.clone(), true));
    assert!(tips.dismiss());
    assert!(!tips.dismiss());
    assert_eq!(tips.active_target(), None);

    assert!(!tips.set_hovered(HoverPlace::Row, tab0.clone(), false));
    assert_eq!(tips.active_target(), None);
    assert!(!tips.set_hovered(HoverPlace::Row, tab0.clone(), true));
    assert_eq!(tips.active_target(), None);
    assert!(!tips.set_hovered(HoverPlace::Row, tab0.clone(), false));
    tips.sync_focus(None);
    assert!(tips.set_hovered(HoverPlace::Row, tab0.clone(), true));
    assert_eq!(tips.active_target(), Some(&tab0));
    assert!(tips.set_hovered(HoverPlace::Row, tab0, false));

    // Exit order 2: focus blurs before hover exits; re-focus stays suppressed until hover leaves.
    tips.sync_focus(Some(tab1.clone()));
    assert!(!tips.set_hovered(HoverPlace::Row, tab1.clone(), true));
    assert!(tips.dismiss());
    tips.sync_focus(None);
    assert_eq!(tips.active_target(), None);
    tips.sync_focus(Some(tab1.clone()));
    assert_eq!(tips.active_target(), None);

    // Different target can still display once suppressed row-hover ends and exposes focused `tab2`.
    tips.sync_focus(Some(tab2.clone()));
    assert!(tips.set_hovered(HoverPlace::Row, tab1.clone(), false));
    assert_eq!(tips.active_target(), Some(&tab2));

    tips.sync_focus(Some(tab1.clone()));
    assert_eq!(tips.active_target(), Some(&tab1));
}

#[gpui_kit::test]
fn popup_only_hover_lifecycle_prevents_ghost_on_dismiss_retarget_and_sync_tabs() {
    let mut tips = RailTips::new();
    let new_cmp = Target::new(RowKey::NewCompare, "New comparison");
    let tab0 = Target::new(RowKey::Tab(0), "Alpha");
    let tab1 = Target::new(RowKey::Tab(1), "Beta");

    // Popup-only dismiss drops overlay_hover and clears orphan latch immediately.
    assert!(tips.set_hovered(HoverPlace::Overlay, tab0.clone(), true));
    assert!(tips.dismiss());
    assert_eq!(tips.active_target(), None);
    assert!(tips.set_hovered(HoverPlace::Row, tab0.clone(), true));
    assert_eq!(tips.active_target(), Some(&tab0));
    assert!(tips.set_hovered(HoverPlace::Row, tab0.clone(), false));

    // Live popup wins over focus; dismissing popup-only falls back cleanly to focused NewCompare.
    tips.sync_focus(Some(new_cmp.clone()));
    assert!(tips.set_hovered(HoverPlace::Overlay, tab0.clone(), true));
    assert_eq!(tips.active_target(), Some(&tab0));
    assert!(tips.dismiss());
    assert_eq!(tips.active_target(), Some(&new_cmp));
    assert!(tips.set_hovered(HoverPlace::Row, tab0.clone(), true));
    assert_eq!(tips.active_target(), Some(&tab0));
    assert!(tips.set_hovered(HoverPlace::Row, tab0.clone(), false));

    // Unmounted popup after `sync_tabs` cannot linger without `on_hover(false)`.
    tips.sync_focus(Some(tab1.clone()));
    assert!(tips.set_hovered(HoverPlace::Overlay, tab0.clone(), true));
    assert_eq!(tips.active_target(), Some(&tab0));
    tips.sync_tabs(&["Alpha".into(), "Beta".into()]);
    assert_eq!(tips.active_target(), Some(&tab1));
}

#[gpui_kit::test]
fn sync_tabs_and_clear_prune_changed_labels_removed_tabs_and_reset_all() {
    let mut tips = RailTips::new();
    let tab0 = Target::new(RowKey::Tab(0), "Alpha");
    let tab1_old = Target::new(RowKey::Tab(1), "Beta");
    let tab1_new = Target::new(RowKey::Tab(1), "Gamma");
    let new_cmp = Target::new(RowKey::NewCompare, "New comparison");
    let rect = Bounds::new(point(px(4.), px(40.)), size(px(48.), px(30.)));

    tips.sync_focus(Some(tab0.clone()));
    assert!(tips.set_hovered(HoverPlace::Row, tab1_old.clone(), true));
    assert!(tips.set_bounds(RowKey::Tab(1), rect));
    assert!(tips.active_anchor().is_some());
    assert!(tips.dismiss());

    // Replacing label at index 1 prunes old row_hover/suppressed and invalidates anchor.
    tips.sync_tabs(&["Alpha".into(), "Gamma".into()]);
    assert_eq!(tips.active_target(), Some(&tab0));
    assert_eq!(tips.active_anchor(), None);
    assert!(tips.set_hovered(HoverPlace::Row, tab1_new.clone(), true));
    assert_eq!(tips.active_target(), Some(&tab1_new));

    // Removing index 1 drops row_hover, leaving kept Tab(0).
    tips.sync_tabs(&["Alpha".into()]);
    assert_eq!(tips.active_target(), Some(&tab0));

    // NewCompare survives empty tab list; `clear()` resets suppression and all targets.
    tips.sync_focus(Some(new_cmp.clone()));
    assert!(tips.set_bounds(RowKey::NewCompare, rect));
    assert!(tips.dismiss());
    tips.sync_tabs(&[]);
    assert_eq!(tips.active_target(), None);

    tips.clear();
    tips.sync_focus(Some(new_cmp.clone()));
    assert_eq!(tips.active_target(), Some(&new_cmp));
    assert_eq!(tips.active_anchor(), None);
}

#[gpui_kit::test]
fn bounds_and_active_anchor_require_matching_active_non_suppressed_positive_bounds() {
    let mut tips = RailTips::new();
    let new_cmp = Target::new(RowKey::NewCompare, "New comparison");
    let tab0 = Target::new(RowKey::Tab(0), "Alpha");
    let r1 = Bounds::new(point(px(4.), px(12.)), size(px(48.), px(30.)));
    let r2 = Bounds::new(point(px(4.), px(14.)), size(px(48.), px(30.)));
    let zero_w = Bounds::new(point(px(4.), px(12.)), size(px(0.), px(30.)));
    let neg_h = Bounds::new(point(px(4.), px(12.)), size(px(48.), px(-2.)));

    assert!(!tips.wants_bounds(RowKey::NewCompare));
    assert!(!tips.set_bounds(RowKey::NewCompare, r1));
    assert_eq!(tips.active_anchor(), None);

    tips.sync_focus(Some(new_cmp.clone()));
    assert!(tips.wants_bounds(RowKey::NewCompare));
    assert!(!tips.wants_bounds(RowKey::Tab(0)));
    assert!(!tips.set_bounds(RowKey::Tab(0), r1));
    assert!(!tips.set_bounds(RowKey::NewCompare, zero_w));
    assert!(!tips.set_bounds(RowKey::NewCompare, neg_h));
    assert_eq!(tips.active_anchor(), None);

    assert!(tips.set_bounds(RowKey::NewCompare, r1));
    assert!(!tips.set_bounds(RowKey::NewCompare, r1));
    assert_eq!(tips.active_anchor(), Some((&new_cmp, r1)));

    assert!(tips.set_bounds(RowKey::NewCompare, r2));
    assert_eq!(tips.active_anchor(), Some((&new_cmp, r2)));

    // Retargeting drops stale anchor even when returning to NewCompare later.
    assert!(tips.set_hovered(HoverPlace::Row, tab0, true));
    assert_eq!(tips.active_anchor(), None);
    assert!(tips.set_hovered(HoverPlace::Row, Target::new(RowKey::Tab(0), "Alpha"), false));
    assert_eq!(tips.active_anchor(), None);

    // Dismiss hides both `wants_bounds` and `active_anchor`.
    assert!(tips.set_bounds(RowKey::NewCompare, r2));
    assert!(tips.dismiss());
    assert!(!tips.wants_bounds(RowKey::NewCompare));
    assert_eq!(tips.active_anchor(), None);
}

gpui_kit::actions!(rail_tip_test, [HarnessEsc, HarnessNext]);

const HARNESS_CTX: &str = "RailTipHarness";

/// Independent component test harness pairing `RailTips` with translated, clipped, bordered rows.
struct Harness {
    tips: RailTips,
    ring_px: f32,
    rail_offset: Point<Pixels>,
    handle: FocusHandle,
    focus_a: FocusHandle,
    focus_b: FocusHandle,
    outside_focus: FocusHandle,
    parent_esc: usize,
}

impl Harness {
    /// Creates a translated clipped-rail harness using the active theme's `ring_px`.
    fn new(rail_offset: Point<Pixels>, cx: &mut Context<Self>) -> Self {
        let ring_px = SidewiseTheme::global(cx).spec.ring_px;
        Self {
            tips: RailTips::new(),
            ring_px,
            rail_offset,
            handle: cx.focus_handle(),
            focus_a: cx.focus_handle().tab_stop(true),
            focus_b: cx.focus_handle().tab_stop(true),
            outside_focus: cx.focus_handle().tab_stop(true),
            parent_esc: 0,
        }
    }

    /// Returns the currently focused row target, if focus is on either harnessed row.
    fn focused_target(&self, window: &Window) -> Option<Target> {
        if self.focus_a.is_focused(window) {
            Some(Target::new(RowKey::NewCompare, "New comparison"))
        } else if self.focus_b.is_focused(window) {
            Some(Target::new(RowKey::Tab(0), "Alpha ↔ Beta"))
        } else {
            None
        }
    }

    /// Updates hovered state for `place` and notifies if the active target changed.
    fn set_hover(
        &mut self,
        (place, target, hovered): (HoverPlace, Target, bool),
        cx: &mut Context<Self>,
    ) {
        if self.tips.set_hovered(place, target, hovered) {
            cx.notify();
        }
    }

    /// Updates captured row outer bounds and notifies when the anchor changed.
    fn set_bounds(&mut self, key: RowKey, outer: Bounds<Pixels>, cx: &mut Context<Self>) {
        if self.tips.set_bounds(key, outer) {
            cx.notify();
        }
    }

    /// Renders one translated, bordered row with `HoverPlace::Row` and padding-box dilation.
    fn row_view(
        &self,
        target: Target,
        focus: &FocusHandle,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let key = target.key;
        let id = match key {
            RowKey::NewCompare => "row-a",
            RowKey::Tab(_) => "row-b",
        };
        let ring_px = self.ring_px;
        let hover_target = target.clone();
        let wants = self.tips.wants_bounds(key);
        let view = cx.weak_entity();
        let probe = canvas(
            move |inner, _, cx| {
                let outer = inner.dilate(px(ring_px));
                let _ = view.update(cx, |this, cx| this.set_bounds(key, outer, cx));
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        div()
            .id(id)
            .relative()
            .track_focus(focus)
            .w_full()
            .h(px(30.))
            .border(px(ring_px))
            .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.set_hover((HoverPlace::Row, hover_target.clone(), *hovered), cx);
            }))
            .test_support()
            .when(wants, |el| el.child(probe))
            .child(target.label)
    }

    /// Builds the clipped 56 px rail container hosting the rows and deferred overlay.
    fn rail_view(
        &self,
        m: &crate::theme_data::palettes::ModeTokens,
        cx: &mut Context<Self>,
    ) -> Div {
        let weak = cx.weak_entity();
        let overlay = self.tips.overlay(
            move |target, hovered, _, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.set_hover((HoverPlace::Overlay, target, hovered), cx);
                });
            },
            m,
        );
        div()
            .key_context(HARNESS_CTX)
            .track_focus(&self.handle)
            .on_action(cx.listener(|this, _: &HarnessEsc, _, cx| {
                if this.tips.dismiss() {
                    cx.notify();
                } else {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &HarnessNext, window, cx| {
                let next = if this.focus_a.is_focused(window) {
                    &this.focus_b
                } else {
                    &this.focus_a
                };
                next.focus(window, cx);
            }))
            .ml(self.rail_offset.x)
            .mt(self.rail_offset.y)
            .flex()
            .flex_col()
            .gap_1()
            .w(px(56.))
            .overflow_hidden()
            .child(self.row_view(
                Target::new(RowKey::NewCompare, "New comparison"),
                &self.focus_a.clone(),
                cx,
            ))
            .child(self.row_view(
                Target::new(RowKey::Tab(0), "Alpha ↔ Beta"),
                &self.focus_b.clone(),
                cx,
            ))
            .children(overlay)
    }
}

impl Render for Harness {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.tips.sync_focus(self.focused_target(window));
        let side = *SidewiseTheme::global(cx);
        let m = if side.mode.is_dark() {
            side.spec.dark
        } else {
            side.spec.light
        };
        let rail = self.rail_view(&m, cx).id("clipped-rail").test_support();
        div()
            .on_action(cx.listener(|this, _: &HarnessEsc, _, _| {
                this.parent_esc += 1;
            }))
            .flex()
            .size_full()
            .child(rail)
            .child(
                div()
                    .id("outside-target")
                    .track_focus(&self.outside_focus)
                    .size(px(20.)),
            )
    }
}

/// Opens a themed harness window with explicit `Harness` escape/next bindings.
fn open_harness(
    cx: &mut TestAppContext,
    palette: PaletteId,
    mode: Mode,
    rail_offset: Point<Pixels>,
) -> (AnyWindowHandle, Entity<Harness>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply_sidewise(palette, mode, cx);
        cx.bind_keys([
            KeyBinding::new("escape", HarnessEsc, Some(HARNESS_CTX)),
            KeyBinding::new("down", HarnessNext, Some(HARNESS_CTX)),
            KeyBinding::new("tab", HarnessNext, Some(HARNESS_CTX)),
        ]);
    });
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: Point::default(),
            size: size(px(640.), px(420.)),
        })),
        ..Default::default()
    };
    let (window, view) = cx
        .update(|cx| {
            gpui_kit::open_window(options, cx, |_, cx| {
                cx.new(|cx| Harness::new(rail_offset, cx))
            })
        })
        .expect("open harness window");
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    (window, view)
}

/// Renders bounded frames until `"rail-tooltip"` presence matches `want` (no fixed sleep).
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
    cx.update_window(window, |_, window, _| {
        assert_eq!(
            window.try_find("rail-tooltip").is_some(),
            want,
            "rail-tooltip visibility did not settle cleanly"
        );
    })
    .expect("window");
}

#[gpui_kit::test]
fn overlay_anchors_10px_right_of_translated_bordered_rows_for_2px_and_3px_rings_escaping_clip(
    cx: &mut TestAppContext,
) {
    for (palette, mode, want_ring) in [
        (PaletteId::Violet, Mode::Light, 2.0_f32),
        (PaletteId::Contrast, Mode::Dark, 3.0_f32),
    ] {
        let (window, view) = open_harness(cx, palette, mode, point(px(18.), px(24.)));
        cx.update_window(window, |_, window, cx| {
            let (ring_px, focus_a) = {
                let h = view.read(cx);
                (h.ring_px, h.focus_a.clone())
            };
            assert_eq!(ring_px, want_ring);
            focus_a.focus(window, cx);
        })
        .expect("window");
        settle_tip(cx, window, true);

        cx.update_window(window, |_, window, _| {
            let rail = window.find("clipped-rail").bounds();
            let row = window.find("row-a").bounds();
            let tip = window.find("rail-tooltip");
            let tb = tip.bounds();
            assert_eq!(tip.role(), Some(Role::Tooltip));
            assert_eq!(tip.label(), Some("New comparison"));
            assert!(tip.visible());
            assert_eq!(
                tb.origin.x,
                row.right() + px(TIP_GAP),
                "tooltip left must equal translated outer row.right() + 10px for {want_ring}px ring"
            );
            assert_eq!(tb.origin.y, row.origin.y);
            assert!(
                tb.right() > rail.right(),
                "deferred overlay must extend past 56px clipped rail"
            );
            let label_text = texts(window)
                .into_iter()
                .find(|t| t.text() == "New comparison")
                .expect("TestTextExt snapshot for tooltip label");
            assert!(label_text.visible(), "tooltip text must not be clipped");
            assert!(label_text.bounds().origin.x >= tb.origin.x);
            assert!(label_text.bounds().right() <= tb.right());
        })
        .expect("window");
    }
}

#[gpui_kit::test]
fn pointer_crosses_10px_gap_bridge_onto_popup_without_dismiss_and_leaves_outside(
    cx: &mut TestAppContext,
) {
    let (window, _) = open_harness(cx, PaletteId::Violet, Mode::Light, point(px(12.), px(16.)));
    let (row_pt, gap_pt, outside_pt) = cx
        .update_window(window, |_, window, _| {
            let row = window.find("row-a").bounds();
            (
                row.center(),
                point(row.right() + px(5.), row.center().y),
                point(px(500.), px(350.)),
            )
        })
        .expect("window");

    let mut vcx = VisualTestContext::from_window(window, cx);
    vcx.simulate_mouse_move(row_pt, None, Modifiers::none());
    settle_tip(cx, window, true);

    let tip_pt = cx
        .update_window(window, |_, window, _| {
            let row = window.find("row-a").bounds();
            let bridge = find(window, &[], &ElementId::Name("rail-tooltip-bridge".into()))
                .expect("bridge")
                .bounds();
            let tip = window.find("rail-tooltip").bounds();
            assert_eq!(bridge.origin.x, row.right(), "bridge flush at row right");
            assert!(tip.origin.x >= bridge.origin.x + px(TIP_GAP));
            tip.center()
        })
        .expect("window");

    VisualTestContext::from_window(window, cx).simulate_mouse_move(gap_pt, None, Modifiers::none());
    settle_tip(cx, window, true);

    VisualTestContext::from_window(window, cx).simulate_mouse_move(tip_pt, None, Modifiers::none());
    settle_tip(cx, window, true);

    VisualTestContext::from_window(window, cx).simulate_mouse_move(
        outside_pt,
        None,
        Modifiers::none(),
    );
    settle_tip(cx, window, false);
}

#[gpui_kit::test]
fn hover_beats_focus_on_live_overlay_and_stationary_keys_keep_hover(cx: &mut TestAppContext) {
    let (window, view) = open_harness(cx, PaletteId::Violet, Mode::Light, point(px(10.), px(10.)));
    let (row_b_pt, outside_pt) = cx
        .update_window(window, |_, window, cx| {
            let focus_a = view.read(cx).focus_a.clone();
            focus_a.focus(window, cx);
            (
                window.find("row-b").bounds().center(),
                point(px(500.), px(350.)),
            )
        })
        .expect("window");
    settle_tip(cx, window, true);

    // Hovering row-b wins over focused row-a; pressing Down/Tab with stationary pointer keeps row-b hover.
    VisualTestContext::from_window(window, cx).simulate_mouse_move(
        row_b_pt,
        None,
        Modifiers::none(),
    );
    settle_tip(cx, window, true);
    cx.update_window(window, |_, window, _| {
        assert_eq!(window.find("rail-tooltip").label(), Some("Alpha ↔ Beta"));
    })
    .expect("window");

    cx.simulate_keystrokes(window, "down");
    settle_tip(cx, window, true);
    cx.simulate_keystrokes(window, "tab");
    settle_tip(cx, window, true);

    // Move onto live overlay while focus is back on row-a: overlay hover wins focus until leaving outside.
    let tip_pt = cx
        .update_window(window, |_, window, _| {
            let tip = window.find("rail-tooltip");
            assert_eq!(tip.label(), Some("Alpha ↔ Beta"));
            tip.bounds().center()
        })
        .expect("window");
    VisualTestContext::from_window(window, cx).simulate_mouse_move(tip_pt, None, Modifiers::none());
    settle_tip(cx, window, true);
    cx.update_window(window, |_, window, _| {
        assert_eq!(window.find("rail-tooltip").label(), Some("Alpha ↔ Beta"));
    })
    .expect("window");

    VisualTestContext::from_window(window, cx).simulate_mouse_move(
        outside_pt,
        None,
        Modifiers::none(),
    );
    settle_tip(cx, window, true);
    cx.update_window(window, |_, window, _| {
        assert_eq!(window.find("rail-tooltip").label(), Some("New comparison"));
    })
    .expect("window");
}

/// Moves the pointer to `pt` inside `window` with no modifiers.
fn move_mouse(cx: &mut TestAppContext, window: AnyWindowHandle, pt: Point<Pixels>) {
    VisualTestContext::from_window(window, cx).simulate_mouse_move(pt, None, Modifiers::none());
}

#[gpui_kit::test]
fn escape_binding_dismisses_active_overlay_and_suppresses_until_all_sources_leave(
    cx: &mut TestAppContext,
) {
    let (window, view) = open_harness(cx, PaletteId::Violet, Mode::Light, point(px(10.), px(10.)));
    let (row_a_pt, outside_pt) = cx
        .update_window(window, |_, window, cx| {
            let focus_a = view.read(cx).focus_a.clone();
            focus_a.focus(window, cx);
            (
                window.find("row-a").bounds().center(),
                point(px(500.), px(350.)),
            )
        })
        .expect("window");
    move_mouse(cx, window, row_a_pt);
    settle_tip(cx, window, true);

    // First Escape consumes to dismiss active tip; second propagates while suppressed.
    cx.simulate_keystrokes(window, "escape");
    settle_tip(cx, window, false);
    cx.update_window(window, |_, _, cx| assert_eq!(view.read(cx).parent_esc, 0))
        .expect("window");
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(view.read(cx).parent_esc, 1);
        assert!(window.try_find("rail-tooltip").is_none());
        let outside_focus = view.read(cx).outside_focus.clone();
        outside_focus.focus(window, cx);
        window.render_frame(cx);
        assert!(window.try_find("rail-tooltip").is_none());
    })
    .expect("window");

    move_mouse(cx, window, outside_pt);
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        let focus_a = view.read(cx).focus_a.clone();
        focus_a.focus(window, cx);
    })
    .expect("window");
    settle_tip(cx, window, true);
}
