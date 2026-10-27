use super::JobHandler;
use super::summary::{JobInfo, Pause};
use crate::job_row::JobRowState;
use crate::prelude::*;
use crate::primitives::{Button, IconButton, IconName};

#[derive(Clone)]
pub(super) struct Actions {
    pub pause: Option<JobHandler>,
    pub resume: Option<JobHandler>,
    pub cancel: Option<JobHandler>,
    pub dismiss: Option<JobHandler>,
    pub is_compact: bool,
}

struct Act {
    name: &'static str,
    label: &'static str,
    glyph: Option<IconName>,
    handler: JobHandler,
}

impl Act {
    fn new(name: &'static str, label: &'static str, handler: JobHandler) -> Self {
        Self {
            name,
            label,
            glyph: None,
            handler,
        }
    }

    fn with_glyph(self, glyph: IconName) -> Self {
        Self {
            glyph: Some(glyph),
            ..self
        }
    }
}

fn pause_or_resume(job: &JobInfo, actions: &Actions) -> Option<Act> {
    let act = match job.pause {
        Pause::Off => {
            Act::new("job-pause", "Pause", actions.pause.clone()?).with_glyph(IconName::Pause)
        }
        Pause::Asked | Pause::On => {
            Act::new("job-resume", "Resume", actions.resume.clone()?).with_glyph(IconName::Play)
        }
    };
    Some(act)
}

fn render(job: &JobInfo, act: Act, is_compact: bool) -> AnyElement {
    let (id, handler) = (job.id, act.handler);
    let click = move |_: &ClickEvent, window: &mut Window, cx: &mut App| handler(id, window, cx);
    if is_compact {
        let spoken = format!("{} {}", act.label, job.name());
        let glyph = act.glyph.unwrap_or(IconName::X);
        return IconButton::new((act.name, id), glyph, spoken)
            .on_click(click)
            .into_any_element();
    }
    Button::new((act.name, id), act.label)
        .when_some(act.glyph, Button::icon)
        .on_click(click)
        .into_any_element()
}

pub(super) fn job_actions(job: &JobInfo, actions: &Actions) -> Vec<AnyElement> {
    let acts = match job.state {
        JobRowState::Queued => vec![
            actions
                .cancel
                .clone()
                .map(|handler| Act::new("job-cancel", "Cancel", handler)),
        ],
        JobRowState::Running => vec![
            pause_or_resume(job, actions),
            actions
                .cancel
                .clone()
                .map(|handler| Act::new("job-cancel", "Cancel", handler)),
        ],
        JobRowState::Failed(_) => vec![
            actions
                .dismiss
                .clone()
                .map(|handler| Act::new("job-dismiss", "Dismiss", handler)),
        ],
        JobRowState::Done | JobRowState::Cancelled => Vec::new(),
    };
    acts.into_iter()
        .flatten()
        .map(|act| render(job, act, actions.is_compact))
        .collect()
}
