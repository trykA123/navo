use std::time::{Duration, Instant};

use gpui_kit::base::TestSupportExt;
use gpui_kit::{
    AnyElement, Context, InteractiveElement, IntoElement, ParentElement, Role,
    StatefulInteractiveElement, Styled, div, px, relative,
};
use rusty_l10n::tr;

use super::ContextMenu;
use super::levels::{LOADING_DELAY, LevelView};
use super::motion;
use super::panel::{PanelLook, RADIUS, chrome};
use crate::tokens::Theme;

const BAR_HEIGHT: f32 = 2.;
const SWEEP: Duration = Duration::from_millis(1400);
const SEGMENT: f32 = 0.35;

/// The loading cue along a panel's top edge; `sweep` is `None` for the static reduced-motion bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Bar {
    pub opacity: f32,
    pub sweep: Option<f32>,
}

impl Bar {
    pub fn is_moving(self) -> bool {
        self.opacity < 1. || self.sweep.is_some()
    }
}

pub(super) fn bar_for(view: &LevelView, now: Instant, fade: Duration, reduce: bool) -> Option<Bar> {
    if !view.is_loading_shown(now) {
        return None;
    }
    let shown = view.loading_since? + LOADING_DELAY;
    let elapsed = now.saturating_duration_since(shown).as_secs_f32();
    let pass = SWEEP.as_secs_f32();
    Some(Bar {
        opacity: motion::progress(shown, now, fade),
        sweep: (!reduce).then(|| (elapsed % pass) / pass),
    })
}

pub(super) fn loading_bar(t: &Theme, level: usize, bar: Bar) -> impl IntoElement {
    let segment = div()
        .id(("menu-loading-sweep", level))
        .test_support()
        .absolute()
        .top_0()
        .h_full()
        .bg(t.colors.accent);
    let segment = match bar.sweep {
        Some(at) => segment
            .w(relative(SEGMENT))
            .left(relative(at * (1. + SEGMENT) - SEGMENT)),
        None => segment.left_0().w_full(),
    };
    div()
        .id(("menu-loading", level))
        .role(Role::ProgressIndicator)
        .aria_label(tr("context-menu-loading"))
        .test_support()
        .absolute()
        .top_0()
        .left(px(RADIUS))
        .right(px(RADIUS))
        .h(px(BAR_HEIGHT))
        .overflow_hidden()
        .opacity(bar.opacity)
        .child(segment)
}

impl ContextMenu {
    pub(super) fn flyout(&self, look: PanelLook, cx: &mut Context<Self>) -> AnyElement {
        let PanelLook {
            level,
            frame,
            theme: t,
            opacity,
            loading,
        } = look;
        chrome(t, level)
            .occlude()
            .left(frame.bounds.origin.x - self.bounds.origin.x)
            .top(frame.bounds.origin.y - self.bounds.origin.y)
            .w(frame.bounds.size.width)
            .h(frame.bounds.size.height)
            .opacity(opacity)
            .child(self.list(&look, cx))
            .children(loading.map(|bar| loading_bar(t, level, bar)))
            .into_any_element()
    }
}
