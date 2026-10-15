#[cfg(test)]
mod tests;

use std::rc::Rc;

use gpui_kit::base::{ObservedElement, TestSupportExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, Div, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, Role, SharedString, Stateful, StatefulInteractiveElement, Styled, Window, div, px,
    relative,
};

use crate::format;
use crate::primitives::{Button, IconName, icon};
use crate::tokens::{self, Theme};

type Handler = Rc<dyn Fn(&mut Window, &mut App)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JobRowState {
    Running,
    Done,
    Failed(SharedString),
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobRowProps {
    pub title: SharedString,
    pub current: Option<SharedString>,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub files_done: u64,
    pub files_total: u64,
    pub state: JobRowState,
}

impl JobRowProps {
    pub fn fraction(&self) -> f32 {
        if self.bytes_total == 0 {
            return 0.;
        }
        let fraction = self.bytes_done as f64 / self.bytes_total as f64;
        fraction.clamp(0., 1.) as f32
    }

    pub fn percent_text(&self) -> String {
        format!("{:.0}%", (self.fraction() * 100.).floor())
    }

    pub fn progress_text(&self) -> String {
        format!(
            "{} of {} · {} of {}",
            format::size(self.bytes_done),
            format::size(self.bytes_total),
            format::bytes(self.files_done),
            format::count(self.files_total, "file", "files"),
        )
    }

    fn status_text(&self) -> String {
        match self.state {
            JobRowState::Running => self.percent_text(),
            JobRowState::Done => "Done".to_owned(),
            JobRowState::Failed(_) => "Failed".to_owned(),
            JobRowState::Cancelled => "Cancelled".to_owned(),
        }
    }

    fn spoken(&self) -> String {
        format!(
            "{}, {}, {}",
            self.title,
            self.status_text(),
            self.progress_text()
        )
    }
}

#[derive(IntoElement)]
pub struct JobRow {
    id: ElementId,
    props: JobRowProps,
    on_cancel: Option<Handler>,
    on_dismiss: Option<Handler>,
}

impl JobRow {
    pub fn new(id: impl Into<ElementId>, props: JobRowProps) -> Self {
        Self {
            id: id.into(),
            props,
            on_cancel: None,
            on_dismiss: None,
        }
    }

    pub fn on_cancel(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_cancel = Some(Rc::new(handler));
        self
    }

    pub fn on_dismiss(mut self, handler: impl Fn(&mut Window, &mut App) + 'static) -> Self {
        self.on_dismiss = Some(Rc::new(handler));
        self
    }
}

fn state_color(t: &Theme, state: &JobRowState) -> Hsla {
    match state {
        JobRowState::Running => t.colors.accent,
        JobRowState::Done => t.colors.ok,
        JobRowState::Failed(_) => t.colors.warn,
        JobRowState::Cancelled => t.colors.faint,
    }
}

fn label(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
) -> ObservedElement<Stateful<Div>> {
    let text = text.into();
    div()
        .id(id)
        .role(Role::Label)
        .aria_label(text.clone())
        .test_support()
        .min_w_0()
        .truncate()
        .child(text)
}

fn bar(t: &Theme, props: &JobRowProps) -> impl IntoElement {
    let color = state_color(t, &props.state);
    let height = px(4.);
    div()
        .id("bar")
        .role(Role::ProgressIndicator)
        .aria_label(props.percent_text())
        .test_support()
        .w_full()
        .h(height)
        .rounded(height / 2.)
        .bg(color.opacity(0.2))
        .child(
            div()
                .h_full()
                .w(relative(props.fraction()))
                .rounded(height / 2.)
                .bg(color),
        )
}

fn detail(t: &Theme, props: &JobRowProps) -> Option<impl IntoElement> {
    let line = div()
        .flex()
        .items_center()
        .gap(t.spacing.sm)
        .font_family(t.typography.mono_font.clone())
        .text_size(t.typography.small);
    match (&props.state, &props.current) {
        (JobRowState::Failed(message), _) => Some(
            line.text_color(t.colors.warn)
                .child(icon(IconName::TriangleAlert, px(11.), t.colors.warn))
                .child(label("message", message.clone())),
        ),
        (JobRowState::Running, Some(current)) => Some(
            line.text_color(t.colors.muted)
                .child(icon(IconName::ArrowRight, px(11.), t.colors.muted))
                .child(label("current", current.clone())),
        ),
        _ => None,
    }
}

fn lines(t: &Theme, props: &JobRowProps) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(3.))
        .flex_1()
        .min_w_0()
        .child(label("title", props.title.clone()).font_weight(FontWeight::SEMIBOLD))
        .children(detail(t, props))
        .child(bar(t, props))
}

fn status(t: &Theme, props: &JobRowProps) -> impl IntoElement {
    let color = match props.state {
        JobRowState::Running => t.colors.text,
        JobRowState::Cancelled => t.colors.muted,
        _ => state_color(t, &props.state),
    };
    div()
        .w(px(72.))
        .flex()
        .justify_end()
        .font_weight(FontWeight::BOLD)
        .text_color(color)
        .child(label("status", props.status_text()))
}

fn action(props: &JobRowProps, on_cancel: Option<Handler>, on_dismiss: Option<Handler>) -> Div {
    let button = match props.state {
        JobRowState::Running => on_cancel.map(|handler| ("cancel", "Cancel", handler)),
        JobRowState::Failed(_) => on_dismiss.map(|handler| ("dismiss", "Dismiss", handler)),
        JobRowState::Done | JobRowState::Cancelled => None,
    };
    div()
        .w(px(72.))
        .flex()
        .justify_end()
        .when_some(button, |slot, (id, text, handler)| {
            slot.child(Button::new(id, text).on_click(move |_, window, cx| handler(window, cx)))
        })
}

impl RenderOnce for JobRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let t = tokens::current(cx);
        let props = &self.props;
        let action = action(props, self.on_cancel, self.on_dismiss);
        div()
            .id(self.id)
            .role(Role::ListItem)
            .aria_label(props.spoken())
            .test_support()
            .flex()
            .items_center()
            .gap(px(14.))
            .px(t.spacing.xl)
            .py(t.spacing.sm)
            .text_size(t.typography.body)
            .child(icon(
                IconName::Files,
                t.spacing.icon,
                state_color(t, &props.state),
            ))
            .child(lines(t, props))
            .child(status(t, props))
            .child(
                div()
                    .w(px(220.))
                    .text_size(t.typography.small)
                    .text_color(t.colors.muted)
                    .child(label("progress", props.progress_text())),
            )
            .child(action)
    }
}
