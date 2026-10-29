use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui_kit::{AsyncWindowContext, Task, WeakEntity};

use super::FerriteWindow;
use crate::prelude::*;

pub(super) type Reply =
    Box<dyn FnOnce(&mut FerriteWindow, &mut Window, &mut Context<FerriteWindow>) + Send>;
pub(super) type Job = Box<dyn FnOnce() -> Reply + Send>;

pub(super) struct Lane {
    sender: UnboundedSender<Job>,
    _task: Task<()>,
    pending: usize,
}

impl Lane {
    pub(super) fn new(window: &mut Window, cx: &mut Context<FerriteWindow>) -> Self {
        let (sender, jobs) = unbounded::<Job>();
        let task = cx.spawn_in(window, async move |this, cx| drain(jobs, this, cx).await);
        Self {
            sender,
            _task: task,
            pending: 0,
        }
    }

    pub(super) fn push(&mut self, job: Job) {
        if self.sender.unbounded_send(job).is_ok() {
            self.pending += 1;
        }
    }

    #[cfg(test)]
    pub(super) fn is_pending(&self) -> bool {
        self.pending != 0
    }
}

impl FerriteWindow {
    pub(super) fn run_off_thread(&mut self, job: Job, _: &mut Window, _: &mut Context<Self>) {
        self.lane.push(job);
    }
}

async fn drain(
    mut jobs: UnboundedReceiver<Job>,
    this: WeakEntity<FerriteWindow>,
    cx: &mut AsyncWindowContext,
) {
    while let Some(job) = jobs.next().await {
        let reply = cx.background_executor().spawn(async move { job() }).await;
        let shown = this.update_in(cx, |view, window, cx| {
            view.lane.pending -= 1;
            reply(view, window, cx);
            cx.notify();
        });
        if shown.is_err() {
            break;
        }
    }
}
