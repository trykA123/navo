use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::TestSupportExt;
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{Animation, AnimationExt, RenderOnce, Role};

use super::summary::{JobInfo, JobsSummary, speed_text, time_text};
use super::{Handler, fade_out, kind_icon, part};
use crate::prelude::*;
use crate::primitives::{IconName, Tooltip, icon};
use crate::tokens::{self, Theme};

const NAME_FROM: f32 = 1100.;
const LEFT_FROM: f32 = 960.;
// The 1.5 px accent ring sits outside, so ring and pill together match a 24 px key.
const PILL_HEIGHT: f32 = 21.;
const BADGE: f32 = 17.;
const GLYPH: f32 = 12.;
const NAME_WIDTH: f32 = 80.;
const BAR_HEIGHT: f32 = 2.;
const SLIDE: f32 = 6.;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PillFit {
    Full,
    NoName,
    Bare,
}

impl PillFit {
    pub fn for_width(width: Pixels) -> Self {
        if width >= px(NAME_FROM) {
            Self::Full
        } else if width >= px(LEFT_FROM) {
            Self::NoName
        } else {
            Self::Bare
        }
    }

    pub fn slot_width(self) -> Pixels {
        match self {
            Self::Full => px(220.),
            Self::NoName => px(140.),
            Self::Bare => px(76.),
        }
    }
}

#[derive(IntoElement)]
pub struct JobPill {
    summary: JobsSummary,
    single: Option<rusty_core::OpKind>,
    fit: PillFit,
    fade: Option<Duration>,
    on_click: Option<Handler>,
}

impl JobPill {
    pub fn new(jobs: &[JobInfo], fit: PillFit) -> Self {
        Self {
            summary: JobsSummary::of(jobs),
            single: match jobs {
                [only] => Some(only.kind),
                _ => None,
            },
            fit,
            fade: None,
            on_click: None,
        }
    }

    #[must_use]
    pub fn fade_out(mut self, over: Duration) -> Self {
        self.fade = Some(over);
        self
    }

    #[must_use]
    pub fn on_click(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

fn badge(t: &Theme, summary: &JobsSummary, single: Option<rusty_core::OpKind>) -> Div {
    let glyph = |name, color| icon(name, px(GLYPH), color).into_any_element();
    let content = match (summary.word(), single) {
        (Some("Failed"), _) => glyph(IconName::TriangleAlert, t.colors.warn),
        (Some("Done"), _) => glyph(IconName::Check, t.colors.ok),
        (Some(_), _) => glyph(IconName::X, t.colors.muted),
        (None, _) if summary.is_paused => div()
            .id("job-pill-paused")
            .test_support()
            .child(glyph(IconName::Pause, t.colors.accent_text))
            .into_any_element(),
        (_, Some(kind)) => glyph(kind_icon(kind), t.colors.accent_text),
        (_, None) => div()
            .text_size(t.typography.tiny)
            .font_weight(FontWeight::SEMIBOLD)
            .child(summary.count.to_string())
            .into_any_element(),
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(BADGE))
        .rounded(t.radii.full)
        .bg(t.surfaces.current)
        .text_color(t.colors.accent_text)
        .child(content)
}

fn bar(t: &Theme, fraction: f32) -> Div {
    div()
        .absolute()
        .left(px(BADGE) + t.spacing.sm)
        .right(t.spacing.xl)
        .bottom(px(BAR_HEIGHT / 2.))
        .h(px(BAR_HEIGHT))
        .rounded(px(BAR_HEIGHT / 2.))
        .overflow_hidden()
        .bg(t.surfaces.progress_track)
        .child(
            div()
                .h_full()
                .w(relative(fraction))
                .bg(t.surfaces.progress_fill),
        )
}

impl JobPill {
    fn spoken(&self) -> String {
        let s = &self.summary;
        let status = match (s.word(), s.is_paused) {
            (Some(word), _) => word.to_owned(),
            (None, true) => format!("Paused, {}", s.percent_text()),
            (None, false) => s.percent_text(),
        };
        let left = s.left.map(|left| format!(", {} left", time_text(left)));
        format!(
            "{}, {status}{}. Show the job card (Ctrl+J)",
            s.name,
            left.unwrap_or_default()
        )
    }

    fn tip(&self) -> String {
        let speed = self.summary.speed.map(speed_text);
        speed.map_or_else(
            || "Show the job card".to_owned(),
            |speed| format!("{speed} · Show the job card"),
        )
    }
}

fn frame(t: &Theme) -> Div {
    div()
        .relative()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .h(px(PILL_HEIGHT))
        .pl(t.spacing.xxs)
        .pr(t.spacing.lg)
        .rounded(t.radii.full)
        .bg(t.surfaces.pill)
        .shadow(t.shadows.pill.clone())
        .overflow_hidden()
        .whitespace_nowrap()
        .cursor_pointer()
}

impl JobPill {
    fn parts(&self, t: &Theme) -> Vec<AnyElement> {
        let s = &self.summary;
        let is_glyph_only = self.fit == PillFit::Bare && s.word().is_some();
        let status = s.word().map_or_else(|| s.percent_text(), str::to_owned);
        let left = s
            .left
            .filter(|_| self.fit != PillFit::Bare && s.is_running)
            .map(|left| format!("{} left", time_text(left)));
        let name = (self.fit == PillFit::Full).then(|| {
            part("job-pill-name", s.name.clone())
                .flex_shrink_1()
                .min_w_0()
                .max_w(px(NAME_WIDTH))
                .truncate()
        });
        let status = (!is_glyph_only).then(|| {
            part("job-pill-status", status)
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(t.surfaces.pill_label)
        });
        let left = left.map(|left| part("job-pill-left", left).text_color(t.colors.muted));
        let mut parts = vec![badge(t, s, self.single).into_any_element()];
        parts.extend(name.map(IntoElement::into_any_element));
        parts.extend(status.map(IntoElement::into_any_element));
        parts.extend(left.map(IntoElement::into_any_element));
        parts.push(bar(t, s.fraction).into_any_element());
        parts
    }
}

impl RenderOnce for JobPill {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let (spoken, tip) = (self.spoken(), self.tip());
        let pill = frame(&t)
            .id("job-pill")
            .role(Role::Button)
            .aria_label(spoken)
            .test_support()
            .children(self.parts(&t))
            .tooltip(move |window, cx| Tooltip::new(tip.clone()).keys("ctrl-j").build(window, cx))
            .when_some(self.on_click, |pill, handler| {
                pill.on_click(move |_, window, cx| handler(window, cx))
            });
        let pill = match (self.fade, cx.reduce_motion()) {
            (_, true) => pill.into_any_element(),
            (Some(over), false) => fade_out(pill, "job-pill-fade", over, |pill| pill),
            (None, false) => {
                let enter = Animation::new(t.motion.base).with_easing(ease_out_cubic);
                pill.with_animation("job-pill-enter", enter, |pill, delta| {
                    pill.left(px(-SLIDE * (1. - delta))).opacity(delta)
                })
                .into_any_element()
            }
        };
        div()
            .flex()
            .h_full()
            .items_center()
            .ml(t.spacing.md)
            .child(pill)
    }
}
