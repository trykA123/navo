mod card;
mod pill;
mod speed;
mod summary;

#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{Role, Stateful};
use rusty_core::OpKind;

use crate::prelude::*;
use crate::primitives::IconName;

pub use card::JobCard;
pub use pill::{JobPill, PillFit};
pub use speed::SpeedMeter;
pub use summary::{JobInfo, JobsSummary, speed_text, time_text};

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;
type JobHandler = Rc<dyn Fn(u64, &mut Window, &mut App)>;

const FADE_FROM: f32 = 0.75;

fn kind_icon(kind: OpKind) -> IconName {
    match kind {
        OpKind::Copy => IconName::Copy,
        OpKind::Move => IconName::FolderInput,
        OpKind::Recycle | OpKind::Delete => IconName::Trash,
    }
}

fn fade_opacity(delta: f32) -> f32 {
    1. - ((delta - FADE_FROM) / (1. - FADE_FROM)).clamp(0., 1.)
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
