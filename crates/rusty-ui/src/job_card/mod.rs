mod actions;
mod card;
mod pill;
mod speed;
mod summary;

#[cfg(test)]
mod tests;

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{Animation, AnimationExt, Role, Stateful};
use rusty_core::OpKind;

use crate::prelude::*;
use crate::primitives::IconName;

pub use card::JobCard;
pub use pill::{JobPill, PillFit};
pub use speed::SpeedMeter;
pub use summary::{JobInfo, JobsSummary, Pause, speed_text, time_text};

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;
type JobHandler = Rc<dyn Fn(u64, &mut Window, &mut App)>;

const FADE_FROM: f32 = 0.75;

fn kind_icon(kind: OpKind) -> IconName {
    match kind {
        OpKind::Copy => IconName::Copy,
        OpKind::Move => IconName::FolderInput,
        OpKind::Recycle | OpKind::Delete => IconName::Trash,
        OpKind::Rename => IconName::Pencil,
        OpKind::Link => IconName::Link,
    }
}

fn fade_opacity(delta: f32) -> f32 {
    1. - ((delta - FADE_FROM) / (1. - FADE_FROM)).clamp(0., 1.)
}

fn fade_out<E: IntoElement + Styled + 'static>(
    element: E,
    id: &'static str,
    over: Duration,
    live: fn(E) -> E,
) -> AnyElement {
    element
        .with_animation(id, Animation::new(over), move |element, delta| {
            if delta >= 1. {
                element.invisible()
            } else {
                live(element).opacity(fade_opacity(delta))
            }
        })
        .into_any_element()
}

fn part(id: impl Into<ElementId>, text: impl Into<SharedString>) -> ObservedElement<Stateful<Div>> {
    let text = text.into();
    div()
        .id(id)
        .role(Role::Label)
        .aria_label(text.clone())
        .test_support()
        .flex_none()
        .child(text)
}
