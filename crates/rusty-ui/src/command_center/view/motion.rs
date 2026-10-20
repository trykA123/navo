use std::time::Duration;

use gpui_kit::base::ObservedElement;
use gpui_kit::base::animation::{ease_in_cubic, ease_out_cubic};
use gpui_kit::{Animation, AnimationExt, AnyElement, Div, IntoElement, Stateful, Styled, px};

use crate::tokens::Motion;

const RISE: f32 = 4.;
pub const LEAVE: Duration = Duration::from_millis(90);

pub(super) fn enter(
    panel: ObservedElement<Stateful<Div>>,
    motion: &Motion,
    (reduce_motion, is_leaving): (bool, bool),
) -> AnyElement {
    if is_leaving {
        let animation = Animation::new(LEAVE).with_easing(ease_in_cubic);
        return panel
            .with_animation("command-center-leave", animation, |panel, delta| {
                panel.opacity(1. - delta)
            })
            .into_any_element();
    }
    let rise = Motion::travel(px(RISE), reduce_motion);
    let animation = Animation::new(motion.fast).with_easing(ease_out_cubic);
    panel
        .with_animation("command-center-enter", animation, move |panel, delta| {
            panel.mt(rise * (delta - 1.)).opacity(delta)
        })
        .into_any_element()
}
