mod button;
mod drawer;
mod line;
mod stops;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{Animation, AnimationExt};
use rusty_core::OpKind;

use crate::activity::{ActivityId, ActivityItem, ActivityKind};
use crate::prelude::*;
use crate::primitives::IconName;
use crate::tokens::{Motion, Theme};

pub use drawer::ActivityDrawer;
pub use line::{ActivityStrip, ChipsShown};
pub use stops::{
    Act, Stop, acts_of, drawer_stops, line_stops, settle, shown_line_stops, step, step_item,
};

gpui_kit::actions!(
    activity_strip,
    [
        Previous,
        Next,
        PreviousItem,
        NextItem,
        PauseOrResume,
        Remove,
        Press,
        Leave
    ]
);

pub const STRIP: &str = "ActivityStrip";
pub const DRAWER: &str = "ActivityDrawer";

pub(crate) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "activity_strip");
}

pub type StopHandler = Rc<dyn Fn(Stop, &mut Window, &mut App)>;
pub type HoverHandler = Rc<dyn Fn(bool, &mut Window, &mut App)>;

const FILL_ALPHA: f32 = 0.11;
const RISE: f32 = 6.;
const BACKGROUND_KEYS: u64 = 1 << 62;

#[must_use]
pub fn key_of(id: ActivityId) -> u64 {
    match id {
        ActivityId::Job(id) => id,
        ActivityId::Background(id) => BACKGROUND_KEYS | id,
        ActivityId::Drag => u64::MAX,
    }
}

fn glyph(t: &Theme, item: &ActivityItem) -> (IconName, Hsla) {
    match (item.kind, item.op) {
        (ActivityKind::Failure, _) => (IconName::TriangleAlert, t.colors.warn),
        (ActivityKind::Done, _) => (IconName::CircleCheck, t.colors.ok),
        (ActivityKind::Drag, _) => (IconName::MousePointer, t.colors.accent_text),
        (ActivityKind::Background, _) => (IconName::LoaderCircle, t.colors.muted),
        (ActivityKind::Job, Some(OpKind::Move)) => (IconName::FolderInput, t.colors.accent_text),
        (ActivityKind::Job, Some(OpKind::Recycle | OpKind::Delete)) => {
            (IconName::Trash, t.colors.accent_text)
        }
        (ActivityKind::Job, _) => (IconName::Copy, t.colors.accent_text),
    }
}

fn fill_color(t: &Theme, item: &ActivityItem) -> Hsla {
    if item.paused {
        t.colors.faint.opacity(FILL_ALPHA)
    } else {
        t.colors.selection
    }
}

fn rise_in<E: IntoElement + Styled + 'static>(
    element: E,
    id: impl Into<ElementId>,
    t: &Theme,
    reduce_motion: bool,
) -> AnyElement {
    let travel = Motion::travel(px(RISE), reduce_motion);
    let enter = Animation::new(t.motion.base).with_easing(ease_out_cubic);
    element
        .with_animation(id, enter, move |element, delta| {
            element.top(travel * (1. - delta)).opacity(delta)
        })
        .into_any_element()
}

fn fade_away<E: IntoElement + Styled + 'static>(
    element: E,
    id: impl Into<ElementId>,
    t: &Theme,
    reduce_motion: bool,
) -> AnyElement {
    let travel = Motion::travel(px(RISE), reduce_motion);
    element
        .with_animation(id, Animation::new(t.motion.fast), move |element, delta| {
            if delta >= 1. {
                element.invisible()
            } else {
                element.top(travel * delta).opacity(1. - delta)
            }
        })
        .into_any_element()
}
