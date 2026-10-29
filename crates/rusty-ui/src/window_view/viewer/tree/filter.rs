use std::time::Duration;

use rusty_structured::{Filtered, Outline, Step};

use super::TreeView;
use crate::prelude::*;
use crate::window_view::viewer::background::spawn_job;

const FILTER_DELAY: Duration = Duration::from_millis(150);

impl TreeView {
    pub(super) fn type_char(&mut self, ch: char, cx: &mut Context<Self>) {
        if self.ready().is_none() {
            return;
        }
        self.filter.push(ch);
        self.schedule_filter(cx);
    }

    pub(super) fn filter_back(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.is_typing_in_bar(window, cx) || self.filter.pop().is_none() {
            cx.propagate();
        } else if self.filter.is_empty() {
            self.clear_filter(cx);
        } else {
            self.schedule_filter(cx);
        }
    }

    pub(super) fn clear_filter(&mut self, cx: &mut Context<Self>) {
        self.filter.clear();
        self.filter_wait = None;
        self.filter_job = None;
        if let Some(ready) = self.ready_mut() {
            ready.outline.set_filter(&ready.tree, "");
        }
        self.follow(Step::Done, cx);
    }

    pub(super) fn schedule_filter(&mut self, cx: &mut Context<Self>) {
        self.filter_job = None;
        self.filter_wait = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FILTER_DELAY).await;
            let _ = this.update(cx, Self::run_filter);
        }));
        cx.notify();
    }

    fn run_filter(&mut self, cx: &mut Context<Self>) {
        self.filter_wait = None;
        let Some(tree) = self.ready().map(|ready| ready.tree.clone()) else {
            return;
        };
        let text = self.filter.clone();
        self.filter_job = Some(spawn_job(
            cx,
            move |_, send| {
                let filtered = Outline::filtered(&tree, &text);
                drop(tree);
                send(filtered);
            },
            Self::filter_done,
        ));
    }

    fn filter_done(&mut self, filtered: Filtered, cx: &mut Context<Self>) {
        if filtered.filter() != self.filter {
            return;
        }
        self.filter_job = None;
        if let Some(ready) = self.ready_mut() {
            ready.outline.show_filtered(filtered);
        }
        self.scroll.scroll_to_top(0);
        self.follow(Step::Done, cx);
    }
}
