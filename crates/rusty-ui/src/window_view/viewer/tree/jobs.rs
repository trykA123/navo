use std::time::Duration;

use rusty_structured::xml::{Entry, XmlIndex, attach_children, build_index, scan_children};
use rusty_structured::{NodeId, Outline, ParseError, Span, Watch};

use super::{Ready, TreeState, TreeView};
use crate::prelude::*;
use crate::window_view::viewer::background::{Job, Throttle, ranged, spawn_job};
use crate::window_view::viewer::body::Host;

const INDEX_EVERY: Duration = Duration::from_millis(100);

pub(super) enum IndexMessage {
    Progress(u64),
    Done(Result<Option<XmlIndex>, ParseError>),
}

pub(super) struct Children {
    node: NodeId,
    result: Result<Vec<Entry>, ParseError>,
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
            let result = build_index(&mut read, len, &mut watch);
            send(IndexMessage::Done(result));
        },
        TreeView::index_message,
    )
}

pub(super) fn start_load(host: &Host, node: NodeId, span: Span, cx: &mut Context<TreeView>) -> Job {
    let mut read = ranged(&host.hooks, &host.location);
    spawn_job(
        cx,
        move |_, send| {
            let result = scan_children(&mut read, span);
            send(Children { node, result });
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
                    Ok(Some(index)) => {
                        let tree = index.tree();
                        TreeState::Ready(Box::new(Ready {
                            outline: Outline::new(&tree),
                            tree,
                            elements: Some(index.elements),
                        }))
                    }
                    Ok(None) => TreeState::Stopped,
                    Err(error) => TreeState::Failed(error),
                };
            }
        }
        cx.notify();
    }

    fn children_loaded(&mut self, loaded: Children, cx: &mut Context<Self>) {
        self.loads.retain(|(node, _)| *node != loaded.node);
        match loaded.result {
            Ok(entries) => {
                if let Some(ready) = self.ready_mut() {
                    attach_children(&mut ready.tree, loaded.node, &entries);
                    ready.outline.loaded(&ready.tree, loaded.node);
                }
            }
            Err(error) => self.note = Some(error.to_string().into()),
        }
        cx.notify();
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
