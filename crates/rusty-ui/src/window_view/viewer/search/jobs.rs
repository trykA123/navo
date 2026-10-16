use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures::StreamExt;
use gpui_kit::Task;

use crate::prelude::*;

struct Cancel(Arc<AtomicBool>);

impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

pub(in crate::window_view::viewer) struct Job {
    _cancel: Cancel,
    _task: Task<()>,
}

pub(in crate::window_view::viewer) struct Throttle {
    last: Instant,
    every: Duration,
}

impl Throttle {
    pub(in crate::window_view::viewer) fn new(every: Duration) -> Self {
        Self {
            last: Instant::now(),
            every,
        }
    }

    pub(in crate::window_view::viewer) fn when_due(&mut self, action: impl FnOnce()) {
        if self.last.elapsed() >= self.every {
            self.last = Instant::now();
            action();
        }
    }
}

pub(in crate::window_view::viewer) fn spawn_job<V: 'static, M: Send + 'static>(
    cx: &mut Context<V>,
    work: impl FnOnce(&AtomicBool, &dyn Fn(M)) + Send + 'static,
    apply: impl Fn(&mut V, M, &mut Context<V>) + 'static,
) -> Job {
    let flag = Arc::new(AtomicBool::new(false));
    let (sender, mut receiver) = futures::channel::mpsc::unbounded::<M>();
    let worker_flag = flag.clone();
    cx.background_spawn(async move {
        work(&worker_flag, &|message| {
            let _ = sender.unbounded_send(message);
        });
    })
    .detach();
    let task = cx.spawn(async move |this, cx| {
        while let Some(message) = receiver.next().await {
            if this
                .update(cx, |view, cx| apply(view, message, cx))
                .is_err()
            {
                break;
            }
        }
    });
    Job {
        _cancel: Cancel(flag),
        _task: task,
    }
}

pub(in crate::window_view::viewer) fn run_detached(work: impl FnOnce() + Send + 'static, cx: &App) {
    cx.background_spawn(async move { work() }).detach();
}
