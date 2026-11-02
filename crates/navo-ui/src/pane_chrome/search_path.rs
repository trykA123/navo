use std::time::Instant;

use gpui_kit::base::TestSupportExt;
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{
    AnyElement, App, ElementId, InteractiveElement, IntoElement, ParentElement, Role,
    StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::tokens;

struct Edge {
    running: bool,
    from: f32,
    since: Instant,
}

impl Edge {
    fn opacity(&self, now: Instant, duration: std::time::Duration) -> (f32, bool) {
        let elapsed = now.saturating_duration_since(self.since).as_secs_f32();
        let delta = (elapsed / duration.as_secs_f32().max(f32::EPSILON)).clamp(0., 1.);
        let target = if self.running { 1. } else { 0. };
        (
            self.from + (target - self.from) * ease_out_cubic(delta),
            delta < 1.,
        )
    }
}

pub(super) fn render(
    id: ElementId,
    (text, running): (gpui_kit::SharedString, bool),
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let t = tokens::current(cx).clone();
    let opacity = edge_opacity(id.clone(), running, window, cx);
    div()
        .id(id)
        .flex_none()
        .relative()
        .mx(t.spacing.md)
        .mb(t.spacing.sm)
        .child(
            div()
                .id("find-path")
                .role(Role::Navigation)
                .aria_label(text.clone())
                .test_support()
                .flex()
                .items_center()
                .h(super::path_bar::bar_height(&t))
                .px(t.spacing.sm)
                .rounded(t.radii.md)
                .bg(t.surfaces.field)
                .text_color(t.colors.muted)
                .child(div().min_w_0().truncate().child(text)),
        )
        .child(
            div()
                .id("find-progress")
                .test_support()
                .absolute()
                .bottom_0()
                .left_0()
                .right_0()
                .h(px(1.))
                .bg(t.colors.accent)
                .opacity(opacity),
        )
        .into_any_element()
}

fn edge_opacity(id: ElementId, running: bool, window: &mut Window, cx: &mut App) -> f32 {
    let t = tokens::current(cx).clone();
    let now = Instant::now();
    let edge = window.use_keyed_state(id, cx, |_, _| Edge {
        running,
        from: 0.,
        since: now,
    });
    let (opacity, fading) = edge.update(cx, |edge, _| {
        let (opacity, _) = edge.opacity(now, t.motion.fade);
        if running != edge.running {
            *edge = Edge {
                running,
                from: opacity,
                since: now,
            };
        }
        edge.opacity(now, t.motion.fade)
    });
    if fading {
        window.request_animation_frame();
    }
    opacity
}
