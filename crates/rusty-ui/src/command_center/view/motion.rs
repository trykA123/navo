use gpui_kit::base::ObservedElement;
use gpui_kit::base::animation::{ease_in_cubic, ease_out_cubic};
use gpui_kit::{Animation, AnimationExt, AnyElement, Div, IntoElement, Stateful, Styled};

use crate::tokens::Motion;

pub(super) fn enter(
    panel: ObservedElement<Stateful<Div>>,
    motion: &Motion,
    is_leaving: bool,
) -> AnyElement {
    if is_leaving {
        let animation = Animation::new(motion.fade).with_easing(ease_in_cubic);
        return panel
            .with_animation("command-center-leave", animation, |panel, delta| {
                panel.opacity(1. - delta)
            })
            .into_any_element();
    }
    let animation = Animation::new(motion.fade).with_easing(ease_out_cubic);
    panel
        .with_animation("command-center-enter", animation, |panel, delta| {
            crate::perf::note(crate::perf::UiFlag::CommandCenter);
            crate::perf::ready(crate::perf::Latency::CommandCenterOpen);
            if delta < 1. {
                crate::perf::note(crate::perf::UiFlag::Animation);
            }
            panel.opacity(delta)
        })
        .into_any_element()
}
