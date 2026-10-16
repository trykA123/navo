use std::rc::Rc;
use std::time::Duration;

use gpui_kit::base::TestSupportExt;
use gpui_kit::base::animation::ease_out_cubic;
use gpui_kit::{Animation, AnimationExt, RenderOnce, Role};

use super::summary::{JobInfo, JobsSummary, speed_text};
use super::{Handler, JobHandler, fade_out, kind_icon, part};
use crate::job_row::JobRowState;
use crate::prelude::*;
use crate::primitives::{Button, IconButton, IconName, Kbd, icon};
use crate::tokens::{self, Theme};

const CARD_WIDTH: f32 = 400.;
const BAR_HEIGHT: f32 = 6.;
const RISE: f32 = 8.;
const FROM_SHRINK: f32 = 100.;

#[derive(IntoElement)]
pub struct JobCard {
    jobs: Vec<JobInfo>,
    fade: Option<Duration>,
    on_shrink: Option<Handler>,
    on_cancel: Option<JobHandler>,
    on_dismiss: Option<JobHandler>,
}

impl JobCard {
    pub fn new(jobs: Vec<JobInfo>) -> Self {
        Self {
            jobs,
            fade: None,
            on_shrink: None,
            on_cancel: None,
            on_dismiss: None,
        }
    }

    #[must_use]
    pub fn fade_out(mut self, over: Duration) -> Self {
        self.fade = Some(over);
        self
    }

    #[must_use]
    pub fn on_shrink(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_shrink = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn on_cancel(mut self, handler: impl Fn(u64, &mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(handler));
        self
    }

    #[must_use]
    pub fn on_dismiss(mut self, handler: impl Fn(u64, &mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }
}

#[derive(Clone)]
struct Actions {
    cancel: Option<JobHandler>,
    dismiss: Option<JobHandler>,
    is_compact: bool,
}

fn row(t: &Theme) -> Div {
    div().flex().items_center().gap(t.spacing.md).min_w_0()
}

fn muted(id: impl Into<ElementId>, text: impl Into<SharedString>, t: &Theme) -> impl IntoElement {
    part(id, text)
        .flex_shrink_1()
        .min_w_0()
        .truncate()
        .text_color(t.colors.muted)
}

fn route(t: &Theme, job: &JobInfo) -> Option<impl IntoElement> {
    let text = job.route()?;
    let (from, to) = match text.split_once(" → ") {
        Some((from, to)) => (Some(from.to_owned()), format!("→ {to}")),
        None => (None, text.clone()),
    };
    let side =
        |text: String, shrink: f32| div().min_w_0().flex_shrink(shrink).truncate().child(text);
    let route = div()
        .id(("job-route", job.id))
        .role(Role::Label)
        .aria_label(text)
        .test_support()
        .flex()
        .flex_shrink_1()
        .min_w_0()
        .gap(t.spacing.xs)
        .text_color(t.colors.muted)
        .children(from.map(|from| side(from, FROM_SHRINK)))
        .child(side(to, 1.));
    Some(route)
}

fn shrink_button(t: &Theme, handler: Option<Handler>) -> impl IntoElement {
    div()
        .id("job-shrink")
        .role(Role::Button)
        .aria_label("Shrink to the F-key bar (Ctrl+J)")
        .test_support()
        .ml_auto()
        .flex()
        .flex_none()
        .items_center()
        .gap(t.spacing.sm)
        .px(t.spacing.xs)
        .py(t.spacing.xxs)
        .rounded(t.radii.md)
        .cursor_pointer()
        .hover(|button| button.bg(t.surfaces.field))
        .children(Kbd::new("job-shrink-key", "ctrl-j"))
        .child(icon(IconName::ChevronDown, t.spacing.icon, t.colors.muted))
        .when_some(handler, |button, handler| {
            button.on_click(move |_, window, cx| handler(window, cx))
        })
}

fn progress_bar(t: &Theme, job: &JobInfo) -> impl IntoElement {
    let color = match job.state {
        JobRowState::Done => t.colors.ok,
        JobRowState::Failed(_) => t.colors.warn,
        JobRowState::Cancelled => t.colors.faint,
        JobRowState::Running => t.surfaces.progress_fill,
    };
    div()
        .id(("job-bar", job.id))
        .role(Role::ProgressIndicator)
        .aria_label(job.percent_text())
        .test_support()
        .w_full()
        .h(px(BAR_HEIGHT))
        .rounded(px(BAR_HEIGHT / 2.))
        .overflow_hidden()
        .bg(t.surfaces.progress_track)
        .child(div().h_full().w(relative(job.fraction())).bg(color))
}

fn detail(t: &Theme, job: &JobInfo) -> Option<Div> {
    let id = job.id;
    let line = row(t).justify_between().text_size(t.typography.small);
    match (&job.state, &job.current) {
        (JobRowState::Failed(message), _) => Some(
            line.text_color(t.colors.warn)
                .child(icon(IconName::TriangleAlert, t.spacing.icon, t.colors.warn))
                .child(
                    part(("job-message", id), message.clone())
                        .flex_1()
                        .min_w_0()
                        .truncate(),
                ),
        ),
        (JobRowState::Running, current) => Some(
            line.child(muted(
                ("job-current", id),
                current.clone().unwrap_or_default(),
                t,
            ))
            .child(part(("job-bytes", id), job.bytes_text()).text_color(t.colors.muted)),
        ),
        _ => None,
    }
}

fn action(job: &JobInfo, actions: &Actions) -> Option<AnyElement> {
    let id = job.id;
    let (name, label, handler) = match job.state {
        JobRowState::Running => ("job-cancel", "Cancel", actions.cancel.clone()?),
        JobRowState::Failed(_) => ("job-dismiss", "Dismiss", actions.dismiss.clone()?),
        JobRowState::Done | JobRowState::Cancelled => return None,
    };
    let click = move |_: &ClickEvent, window: &mut Window, cx: &mut App| handler(id, window, cx);
    if actions.is_compact {
        let spoken = format!("{label} {}", job.name());
        return Some(
            IconButton::new((name, id), IconName::X, spoken)
                .on_click(click)
                .into_any_element(),
        );
    }
    Some(
        Button::new((name, id), label)
            .on_click(click)
            .into_any_element(),
    )
}

fn foot(t: &Theme, job: &JobInfo, actions: &Actions) -> Div {
    let color = match job.state {
        JobRowState::Done => t.colors.ok,
        _ => t.colors.muted,
    };
    row(t)
        .text_size(t.typography.small)
        .child(
            part(("job-status", job.id), job.status_text())
                .flex_1()
                .min_w_0()
                .truncate()
                .text_color(color),
        )
        .children(action(job, actions))
}

fn body(t: &Theme, job: &JobInfo, actions: &Actions) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .children(detail(t, job))
        .child(progress_bar(t, job))
        .child(foot(t, job, actions))
}

fn job_row(t: &Theme, job: &JobInfo, actions: &Actions) -> impl IntoElement {
    let title = row(t)
        .child(icon(
            kind_icon(job.kind),
            t.spacing.icon,
            t.colors.accent_text,
        ))
        .child(part(("job-name", job.id), job.name()).font_weight(FontWeight::SEMIBOLD))
        .children(route(t, job));
    div()
        .id(("job-row", job.id))
        .role(Role::ListItem)
        .aria_label(format!("{}, {}", job.name(), job.status_text()))
        .test_support()
        .flex()
        .flex_col()
        .gap(t.spacing.sm)
        .pt(t.spacing.lg)
        .border_t_1()
        .border_color(t.surfaces.card_edge)
        .child(title)
        .child(body(t, job, actions))
}

impl JobCard {
    fn head(&self, t: &Theme, summary: &JobsSummary) -> Div {
        let shrink = shrink_button(t, self.on_shrink.clone());
        if let [job] = self.jobs.as_slice() {
            let (glyph, color) = match job.state {
                JobRowState::Done => (IconName::Check, t.colors.ok),
                JobRowState::Failed(_) => (IconName::TriangleAlert, t.colors.warn),
                _ => (kind_icon(job.kind), t.colors.accent_text),
            };
            return row(t)
                .child(icon(glyph, t.spacing.icon, color))
                .child(part("job-heading", job.heading()).font_weight(FontWeight::SEMIBOLD))
                .children(route(t, job))
                .child(shrink);
        }
        let mut parts = vec![summary.percent_text()];
        parts.extend(summary.speed.map(speed_text));
        row(t)
            .child(icon(IconName::List, t.spacing.icon, t.colors.accent_text))
            .child(part("job-heading", summary.name.clone()).font_weight(FontWeight::SEMIBOLD))
            .child(muted("job-total", parts.join(" · "), t))
            .child(shrink)
    }

    fn spoken(&self, summary: &JobsSummary) -> String {
        match self.jobs.as_slice() {
            [job] => format!("{}, {}", job.heading(), job.status_text()),
            _ => format!("{}, {}", summary.name, summary.percent_text()),
        }
    }
}

impl RenderOnce for JobCard {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx).clone();
        let summary = JobsSummary::of(&self.jobs);
        let actions = Actions {
            cancel: self.on_cancel.clone(),
            dismiss: self.on_dismiss.clone(),
            is_compact: self.jobs.len() > 1,
        };
        let content: Vec<AnyElement> = match self.jobs.as_slice() {
            [job] => vec![body(&t, job, &actions).into_any_element()],
            jobs => jobs
                .iter()
                .map(|job| job_row(&t, job, &actions).into_any_element())
                .collect(),
        };
        let card = div()
            .id("job-card")
            .role(Role::Status)
            .aria_label(self.spoken(&summary))
            .test_support()
            .relative()
            .flex()
            .flex_col()
            .gap(t.spacing.lg)
            .w(px(CARD_WIDTH))
            .px(t.spacing.xxl)
            .py(t.spacing.xl)
            .rounded(t.radii.job_card)
            .bg(t.surfaces.job_card)
            .shadow(t.shadows.job_card.clone())
            .text_size(t.typography.body)
            .text_color(t.colors.text)
            .child(self.head(&t, &summary))
            .children(content);
        match (self.fade, cx.reduce_motion()) {
            (_, true) => card.occlude().into_any_element(),
            (Some(over), false) => fade_out(card, "job-card-fade", over, |card| card.occlude()),
            (None, false) => {
                let enter = Animation::new(t.motion.base).with_easing(ease_out_cubic);
                card.with_animation("job-card-enter", enter, |card, delta| {
                    card.occlude().top(px(RISE * (1. - delta))).opacity(delta)
                })
                .into_any_element()
            }
        }
    }
}
