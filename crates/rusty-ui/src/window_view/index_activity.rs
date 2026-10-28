use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures::StreamExt;
use futures::channel::mpsc::{self, Receiver, Sender};
use gpui_kit::{AsyncApp, Subscription, Task, WeakEntity};

use super::FerriteWindow;
use crate::command_center::{IndexHooks, Indexing};
use crate::prelude::*;
use crate::settings::{Field, Value};

const POLL: Duration = Duration::from_millis(500);
const INDEX_ACTIVITY: u64 = 0;

pub(super) struct Activity {
    jobs: Arc<Jobs>,
    receiver: Option<Receiver<()>>,
    poll: Option<Task<()>>,
    previous: Option<Indexing>,
    enabled: bool,
    _listener: Option<Task<()>>,
    _mode: Option<Subscription>,
}

struct Jobs {
    running: AtomicUsize,
    revision: AtomicUsize,
    sender: Mutex<Sender<()>>,
}

struct Running<'a>(&'a Jobs);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        self.0.running.fetch_sub(1, Ordering::SeqCst);
        self.0.notify();
    }
}

impl Jobs {
    fn notify(&self) {
        self.revision.fetch_add(1, Ordering::SeqCst);
        let _ = self
            .sender
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .try_send(());
    }

    fn run<T>(&self, job: impl FnOnce() -> T) -> T {
        self.running.fetch_add(1, Ordering::SeqCst);
        self.notify();
        let _running = Running(self);
        job()
    }
}

impl Activity {
    pub(super) fn new(hooks: &mut IndexHooks) -> Self {
        let (sender, receiver) = mpsc::channel(1);
        let jobs = Arc::new(Jobs {
            running: AtomicUsize::new(0),
            revision: AtomicUsize::new(0),
            sender: Mutex::new(sender),
        });
        let start = hooks.start.clone();
        let started = jobs.clone();
        hooks.start = Arc::new(move |options| started.run(|| start(options)));
        let host = hooks.host.clone();
        let searched = jobs.clone();
        hooks.host = Arc::new(move |options, query, cancel, status| {
            searched.run(|| host(options, query, cancel, status))
        });
        Self {
            jobs,
            receiver: Some(receiver),
            poll: None,
            previous: None,
            enabled: false,
            _listener: None,
            _mode: None,
        }
    }

    #[cfg(test)]
    pub(super) fn is_polling(&self) -> bool {
        self.poll.is_some()
    }
}

impl FerriteWindow {
    pub(super) fn poll_index_activity(cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = this.update(cx, Self::listen_index_activity);
        });
    }

    fn listen_index_activity(&mut self, cx: &mut Context<Self>) {
        let Some(mut receiver) = self.command_center.index_activity.receiver.take() else {
            return;
        };
        self.command_center.index_activity._mode = Some(cx.observe_self(Self::sync_index_activity));
        self.command_center.index_activity._listener = Some(cx.spawn(async move |this, cx| {
            while receiver.next().await.is_some()
                && this.update(cx, Self::restart_index_activity).is_ok()
            {}
        }));
        self.sync_index_activity(cx);
    }

    fn sync_index_activity(&mut self, cx: &mut Context<Self>) {
        let enabled = self.index_activity_enabled();
        if self.command_center.index_activity.enabled == enabled {
            return;
        }
        self.command_center.index_activity.enabled = enabled;
        self.restart_index_activity(cx);
    }

    pub(super) fn restart_index_activity(&mut self, cx: &mut Context<Self>) {
        if !self.index_activity_enabled() {
            self.command_center.index_activity.poll = None;
            self.set_index_activity(None, cx);
            return;
        }
        if self.command_center.index_activity.poll.is_some() {
            return;
        }
        self.command_center.index_activity.poll = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(POLL).await;
            while Self::refresh_index_activity(&this, cx).await {
                cx.background_executor().timer(POLL).await;
            }
        }));
    }

    async fn refresh_index_activity(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> bool {
        let Ok((status, revision)) = this.read_with(cx, |view, _| {
            (
                view.index_activity_enabled()
                    .then(|| view.command_center.hooks.index.status.clone()),
                view.command_center
                    .index_activity
                    .jobs
                    .revision
                    .load(Ordering::SeqCst),
            )
        }) else {
            return false;
        };
        let indexing = match status {
            Some(status) => cx.background_spawn(async move { status() }).await,
            None => None,
        };
        this.update(cx, |view, cx| {
            let jobs = &view.command_center.index_activity.jobs;
            let active = view.index_activity_enabled()
                && (indexing.is_some()
                    || jobs.running.load(Ordering::SeqCst) > 0
                    || jobs.revision.load(Ordering::SeqCst) != revision);
            view.set_index_activity(indexing, cx);
            if !active {
                view.command_center.index_activity.poll = None;
            }
            active
        })
        .unwrap_or(false)
    }

    fn set_index_activity(&mut self, indexing: Option<Indexing>, cx: &mut Context<Self>) {
        let indexing = indexing.filter(|_| self.index_activity_enabled());
        if self.command_center.index_activity.previous == indexing {
            return;
        }
        match &indexing {
            Some(indexing) => {
                self.job_ui
                    .activity
                    .set_background(INDEX_ACTIVITY, indexing.text(), None);
            }
            None => self.job_ui.activity.end_background(INDEX_ACTIVITY),
        }
        self.command_center.index_activity.previous = indexing;
        self.refresh_activity(cx);
    }

    fn index_activity_enabled(&self) -> bool {
        !matches!(self.values.get(Field::SearchIndex), Value::Pick(3))
    }
}
