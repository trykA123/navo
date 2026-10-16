use std::sync::Arc;
use std::time::Duration;

use rusty_structured::xml::{Children, attach_children, build_index, scan_children};
use rusty_structured::{NodeId, ParseError, Span, Watch};

use super::{Ready, TreeState, TreeView};
use crate::prelude::*;
use crate::window_view::viewer::background::{Job, Throttle, ranged, spawn_job};
use crate::window_view::viewer::body::Host;

const INDEX_EVERY: Duration = Duration::from_millis(100);
const ATTACH_RETRY: Duration = Duration::from_millis(20);
const TOO_MANY: &str = "Too many values to show: some children are hidden.";

pub(super) enum IndexMessage {
    Progress(u64),
    Done(Result<Option<Box<Ready>>, ParseError>),
}

pub(super) struct Loaded {
    node: NodeId,
    result: Result<Option<Children>, ParseError>,
}

pub(super) fn start_index(host: &Host, len: u64, cx: &mut Context<TreeView>) -> Job {
    let mut read = ranged(&host.hooks, &host.location);
    spawn_job(
        cx,
        move |cancel, send| {
            let mut throttle = Throttle::new(INDEX_EVERY);
            let mut progress = |count: u64| {
                throttle.when_due(|| send(IndexMessage::Progress(count)));
            };
            let mut watch = Watch {
                cancel,
                progress: &mut progress,
            };
            let result = build_index(&mut read, len, &mut watch).map(|index| {
                index.map(|index| Box::new(Ready::new(index.tree(), Some(index.elements))))
            });
            send(IndexMessage::Done(result));
        },
        TreeView::index_message,
    )
}

pub(super) fn start_load(host: &Host, node: NodeId, span: Span, cx: &mut Context<TreeView>) -> Job {
    let mut read = ranged(&host.hooks, &host.location);
    spawn_job(
        cx,
        move |cancel, send| {
            let result = scan_children(&mut read, span, cancel);
            send(Loaded { node, result });
        },
        TreeView::children_loaded,
    )
}

impl TreeView {
    fn index_message(&mut self, message: IndexMessage, cx: &mut Context<Self>) {
        match message {
            IndexMessage::Progress(count) => {
                if let TreeState::Indexing(shown) = &mut self.state {
                    *shown = count;
                }
            }
            IndexMessage::Done(result) => {
                self.index_job = None;
                self.state = match result {
                    Ok(Some(ready)) => TreeState::Ready(ready),
                    Ok(None) => TreeState::Stopped,
                    Err(error) => TreeState::Failed(error),
                };
                self.sync_path();
            }
        }
        cx.notify();
    }

    fn children_loaded(&mut self, loaded: Loaded, cx: &mut Context<Self>) {
        self.loads.retain(|(node, _)| *node != loaded.node);
        match loaded.result {
            Ok(Some(children)) => {
                self.filter_job = None;
                self.pending.push((loaded.node, children));
                self.attach_pending(cx);
            }
            Ok(None) => {}
            Err(error) => self.note = Some(error.to_string().into()),
        }
        cx.notify();
    }

    fn attach_pending(&mut self, cx: &mut Context<Self>) {
        let TreeState::Ready(ready) = &mut self.state else {
            return;
        };
        let Some(tree) = Arc::get_mut(&mut ready.tree) else {
            self.retry_attach(cx);
            return;
        };
        let mut is_complete = true;
        for (node, children) in self.pending.drain(..) {
            is_complete &= attach_children(tree, node, &children);
            ready.outline.loaded(tree, node);
        }
        self.sync_path();
        if !is_complete {
            self.note = Some(TOO_MANY.into());
        }
        if self.is_filtering() {
            self.schedule_filter(cx);
        }
        cx.notify();
    }

    fn retry_attach(&mut self, cx: &mut Context<Self>) {
        if self.attach_wait.is_some() {
            return;
        }
        self.attach_wait = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(ATTACH_RETRY).await;
            let _ = this.update(cx, |view, cx| {
                view.attach_wait = None;
                view.attach_pending(cx);
            });
        }));
    }

    pub(super) fn stop_indexing(&mut self, cx: &mut Context<Self>) -> bool {
        if self.index_job.take().is_none() {
            return false;
        }
        self.state = TreeState::Stopped;
        cx.notify();
        true
    }
}
