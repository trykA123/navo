//! Setup's Installing page: the bar, the elapsed caption and the cross-fading tips.

use std::time::Duration;

use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::{
    Animation, AnimationExt, InteractiveElement, Role, SharedString, Stateful,
    StatefulInteractiveElement,
};

use super::pages_a::{Ctx, column};
use super::pages_b::key_chip;
use crate::{AnyElement, Div, IntoElement, ParentElement, Styled, div, px, relative};

type Tip = ObservedElement<Stateful<Div>>;

const BAR_HEIGHT: f32 = 6.;
const TIP_CYCLE: Duration = Duration::from_secs(16);
const TIP_AREA_HEIGHT: f32 = 24.;
const BAR_PULSE: Duration = Duration::from_millis(2400);
/// Shorter than the runner's 250 ms poll, so each new share settles before the next one.
const FILL_EASE: Duration = Duration::from_millis(200);

/// Formats a duration as minutes and seconds, `3:12`.
fn clock(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// Builds the bar: a verified share fills it, and no share pulses instead.
fn bar(c: &Ctx, percent: Option<u8>) -> impl IntoElement {
    let track = div()
        .id("installing-bar")
        .role(Role::ProgressIndicator)
        .aria_label(c.strings.installing_bar.to_string())
        .test_support()
        .w_full()
        .h(px(BAR_HEIGHT))
        .rounded(px(BAR_HEIGHT / 2.))
        .overflow_hidden()
        .bg(c.t.surfaces.progress_track);
    match percent {
        Some(percent) => track
            .aria_value(share_text(c, percent))
            .child(fill(c, f32::from(percent) / 100.)),
        None => track.child(pulse(c)),
    }
}

/// The `NN %` text the bar and its caption both name.
fn share_text(c: &Ctx, percent: u8) -> String {
    c.strings
        .installing_percent
        .replace("{percent}", &percent.to_string())
}

/// The verified share of the track, eased into place; reduced motion shows it at once.
fn fill(c: &Ctx, fraction: f32) -> AnyElement {
    let fill = div()
        .id("installing-fill")
        .test_support()
        .h_full()
        .w(relative(fraction))
        .rounded(px(BAR_HEIGHT / 2.))
        .bg(c.t.surfaces.progress_fill);
    if c.reduce_motion {
        return fill.into_any_element();
    }
    fill.with_animation(
        "installing-bar-fill",
        Animation::new(FILL_EASE).with_easing(ease_out_cubic),
        move |fill, delta| fill.w(relative(fraction * delta)),
    )
    .into_any_element()
}

/// The indeterminate fill: a slow fade, standing still under reduced motion.
fn pulse(c: &Ctx) -> AnyElement {
    let fill = div()
        .h_full()
        .w(relative(0.4))
        .rounded(px(BAR_HEIGHT / 2.))
        .bg(c.t.surfaces.progress_fill);
    if c.reduce_motion {
        return fill.into_any_element();
    }
    fill.with_animation(
        "installing-bar-pulse",
        Animation::new(BAR_PULSE).repeat(),
        |fill, delta| fill.opacity(0.55 + 0.45 * (delta * std::f32::consts::TAU).sin().abs()),
    )
    .into_any_element()
}

/// The elapsed caption, with the verified share beside it when there is one.
fn timing(c: &Ctx, elapsed: Duration, percent: Option<u8>) -> Div {
    let text = c
        .strings
        .installing_elapsed
        .replace("{time}", &clock(elapsed));
    let row = div().flex().items_center().gap(c.t.spacing.sm).child(
        div()
            .id("installing-elapsed")
            .role(Role::Status)
            .aria_label(text.clone())
            .test_support()
            .text_size(c.t.typography.body)
            .text_color(c.t.colors.text)
            .child(text),
    );
    let Some(percent) = percent else {
        return row;
    };
    let share = share_text(c, percent);
    row.child(
        div()
            .id("installing-percent")
            .role(Role::Status)
            .aria_label(share.clone())
            .test_support()
            .text_size(c.t.typography.body)
            .text_color(c.t.colors.muted)
            .child(share),
    )
}

/// Opacity of tip `index` at `delta` (0..1 of the cycle): a short fade in, a hold, a fade out.
fn tip_opacity(index: usize, tips: usize, delta: f32) -> f32 {
    const EDGE: f32 = 0.06;
    let local = (delta * tips as f32 - index as f32).rem_euclid(tips as f32);
    match local {
        l if l < EDGE => l / EDGE,
        l if l < 1. - EDGE => 1.,
        l if l < 1. => (1. - l) / EDGE,
        _ => 0.,
    }
}

/// Builds one tip line: the key in a small mono chip, then its text.
fn tip_line(c: &Ctx, id: SharedString, key: &SharedString, text: &SharedString) -> Tip {
    let t = c.t;
    div()
        .id(id)
        .role(Role::Label)
        .aria_label(format!("{key}: {text}"))
        .test_support()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .text_size(t.typography.small)
        .text_color(t.colors.text)
        .child(key_chip(c, key))
        .child(text.clone())
}

/// Builds "While you wait": the tips cross-fade every 4 s; reduced motion shows the first.
fn tips(c: &Ctx) -> impl IntoElement {
    let t = c.t;
    let all = &c.strings.installing_tips;
    let shown = if c.reduce_motion { 1 } else { all.len() };
    let count = all.len();
    let lines = all
        .iter()
        .take(shown)
        .enumerate()
        .map(|(index, (key, text))| {
            let id = SharedString::from(format!("installing-tip-{index}"));
            let line = tip_line(c, id.clone(), key, text);
            if c.reduce_motion {
                return line.into_any_element();
            }
            line.opacity(0.)
                .with_animation(
                    id,
                    Animation::new(TIP_CYCLE).repeat(),
                    move |line, delta| line.opacity(tip_opacity(index, count, delta)),
                )
                .into_any_element()
        });
    div()
        .id("installing-tips")
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .pt(t.spacing.md)
        .border_t_1()
        .border_color(t.colors.line)
        .child(
            div()
                .text_size(t.typography.small)
                .text_color(t.colors.muted)
                .child(c.strings.installing_tips_title.clone()),
        )
        .child(div().relative().h(px(TIP_AREA_HEIGHT)).children(lines))
}

/// Installing: bar, elapsed time and tips; after 180 s the still-working text with the log. No button.
pub(super) fn installing(c: &Ctx) -> AnyElement {
    let progress = c.view.progress;
    let elapsed = progress.map_or(Duration::ZERO, |p| p.elapsed);
    let still = progress.is_some_and(|p| p.still_working);
    let percent = progress.and_then(|p| p.percent);
    let mut page = column(c, &c.strings.installing_title)
        .child(bar(c, percent))
        .child(timing(c, elapsed, percent));
    if still {
        let log = c.view.log_paths.first().map(|p| p.display().to_string());
        let text = c
            .strings
            .installing_still
            .replace("{log}", &log.unwrap_or_default());
        page = page.child(
            div()
                .id("installing-still")
                .role(Role::Status)
                .aria_label(text.clone())
                .test_support()
                .text_size(c.t.typography.body)
                .text_color(c.t.colors.muted)
                .child(text),
        );
    } else {
        page = page.child(tips(c));
    }
    page.into_any_element()
}
