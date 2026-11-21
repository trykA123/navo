//! Motion tokens and the reduced-motion gate (NC68): every Sidewise animation goes through
//! `anim`, so one accessibility flag stops all of them. Nothing loops and nothing overshoots.

use std::time::Duration;

use gpui_kit::base::animation::cubic_bezier;
use gpui_kit::{Animation, App, ElementId, ReadGlobal};

use crate::theme::SidewiseTheme;

/// The prototype's `--ease: cubic-bezier(.2,.8,.2,1)`: a fast start that decelerates to a stop.
/// gpui clamps the curve to `0..=1`, so no transition can overshoot or bounce.
pub fn ease_out_quint(t: f32) -> f32 {
    cubic_bezier(0.2, 0.8, 0.2, 1.0)(t)
}

/// One one-shot transition, or `None` when the user asked for reduced motion.
///
/// `id` is the element id the caller hands to `with_animation`; `Animation` carries no id of its
/// own. Nothing here repeats, so no animation can flash.
pub fn anim(_id: impl Into<ElementId>, duration_ms: u64, cx: &App) -> Option<Animation> {
    if cx.reduce_motion() {
        return None;
    }
    Some(Animation::new(Duration::from_millis(duration_ms)).with_easing(ease_out_quint))
}

/// `--dur-hover`: colour and transform on a hovered control.
pub fn hover(cx: &App) -> Duration {
    Duration::from_millis(SidewiseTheme::global(cx).motion.hover_ms)
}

/// `--dur-menu`: a menu or popover opening.
pub fn menu(cx: &App) -> Duration {
    Duration::from_millis(SidewiseTheme::global(cx).motion.menu_ms)
}

/// `--dur-theme`: a palette or mode change.
pub fn theme(cx: &App) -> Duration {
    Duration::from_millis(SidewiseTheme::global(cx).motion.theme_ms)
}

/// `--dur-screen`: a switch between the start screen and a session.
pub fn screen(cx: &App) -> Duration {
    Duration::from_millis(SidewiseTheme::global(cx).motion.screen_ms)
}

/// `--dur-row`: a row entering or leaving a list.
pub fn row(cx: &App) -> Duration {
    Duration::from_millis(SidewiseTheme::global(cx).motion.row_ms)
}

/// `--dur-side`: the sidebar collapsing to the rail and back.
pub fn side(cx: &App) -> Duration {
    Duration::from_millis(SidewiseTheme::global(cx).motion.side_ms)
}

/// `--dur-sheet`: a sheet or dialog opening.
pub fn sheet(cx: &App) -> Duration {
    Duration::from_millis(SidewiseTheme::global(cx).motion.sheet_ms)
}
