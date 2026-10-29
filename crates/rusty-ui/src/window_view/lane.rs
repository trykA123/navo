use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui_kit::{AsyncWindowContext, Task, WeakEntity};

use super::FerriteWindow;
use crate::prelude::*;

pub(super) type Reply =
    Box<dyn FnOnce(&mut FerriteWindow, &mut Window, &mut Context<FerriteWindow>) + Send>;
pub(super) type Job = Box<dyn FnOnce() -> Reply + Send>;

#[derive(Default)]
pub(super) struct Lane {
    sender: Option<UnboundedSender<Job>>,
    task: Option<Task<()>>,
}

impl FerriteWindow {
    pub(super) fn run_off_thread(&mut self, job: Job, window: &mut Window, cx: &mut Context<Self>) {
        let lane = &mut self.lane;
        let sender = lane.sender.get_or_insert_with(|| {
            let (sender, jobs) = unbounded::<Job>();
            lane.task = Some(cx.spawn_in(window, async move |this, cx| {
                drain(jobs, this, cx).await;
            }));
            sender
        });
        let _ = sender.unbounded_send(job);
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
            reply(view, window, cx);
            cx.notify();
        });
        if shown.is_err() {
            break;
        }
    }
}
