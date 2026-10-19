use std::time::Duration;

use gpui_kit::{Subscription, Task};
use rusty_core::Location;

use super::FerriteWindow;
use crate::prelude::*;

const REST_AFTER: Duration = Duration::from_millis(150);

pub(super) struct Rest {
    target: Option<Location>,
    wait: Option<Task<()>>,
    _changes: Subscription,
}

impl Rest {
    pub(super) fn observe(cx: &mut Context<FerriteWindow>) -> Self {
        Self {
            target: None,
            wait: None,
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
                let _ = this.update(cx, |view, _| view.prefetch_rested(location));
            })
        });
    }

    fn prefetch_rested(&mut self, location: Location) {
        self.rest.wait = None;
        if self.rest.target.as_ref() == Some(&location) {
            self.browser.prefetch(location);
        }
    }
}
