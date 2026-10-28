use std::collections::VecDeque;
use std::time::{Duration, Instant};

use gpui_kit::{Subscription, Task};
use rusty_core::Location;

use super::FerriteWindow;
use crate::pane_chrome::{DriveIcon, DriveInfo, current_drive};
use crate::prelude::*;

const REST_AFTER: Duration = Duration::from_millis(150);
const PREFETCHES_PER_WINDOW: usize = 4;
const BUDGET_SPAN: Duration = Duration::from_secs(1);

#[derive(Default)]
pub(super) struct Budget(VecDeque<Instant>);

impl Budget {
    pub(super) fn has_room(&mut self, now: Instant) -> bool {
        while self
            .0
            .front()
            .is_some_and(|at| now.saturating_duration_since(*at) >= BUDGET_SPAN)
        {
            self.0.pop_front();
        }
        self.0.len() < PREFETCHES_PER_WINDOW
    }

    pub(super) fn spend(&mut self, now: Instant) {
        self.0.push_back(now);
    }
}

pub(super) fn is_far(drives: &[DriveInfo], location: &Location) -> bool {
    let text = location.as_str();
    if text.starts_with(r"\\") || text.starts_with("//") {
        return true;
    }
    let drive = current_drive(drives, location).and_then(|index| drives.get(index));
    match drive {
        Some(drive) => matches!(
            drive.icon,
            DriveIcon::Network | DriveIcon::Removable | DriveIcon::Optical
        ),
        None => has_drive_letter(text),
    }
}

fn has_drive_letter(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphabetic()) && chars.next() == Some(':')
}

pub(super) struct Rest {
    target: Option<Location>,
    wait: Option<Task<()>>,
    budget: Budget,
    _changes: Subscription,
}

impl Rest {
    pub(super) fn observe(cx: &mut Context<FerriteWindow>) -> Self {
        Self {
            target: None,
            wait: None,
            budget: Budget::default(),
            _changes: cx.observe_self(|view, cx| {
                view.watch_listings(cx);
                view.watch_rest(cx);
            }),
        }
    }
}

impl FerriteWindow {
    fn watch_rest(&mut self, cx: &mut Context<Self>) {
        let target = self.browser.cursor_folder();
        if target == self.rest.target {
            return;
        }
        self.browser.leave_prefetches(target.as_ref());
        self.rest.target.clone_from(&target);
        self.rest.wait = target.map(|location| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(REST_AFTER).await;
                let _ = this.update(cx, |view, cx| view.prefetch_rested(location, cx));
            })
        });
    }

    fn prefetch_rested(&mut self, location: Location, cx: &Context<Self>) {
        self.rest.wait = None;
        let now = cx.background_executor().now();
        let is_allowed = self.rest.target.as_ref() == Some(&location)
            && !is_far(&self.drives.list, &location)
            && self.rest.budget.has_room(now);
        if is_allowed && self.browser.prefetch(location) {
            self.rest.budget.spend(now);
        }
    }
}
