use std::ops::ControlFlow;
use std::sync::mpsc::{self, Receiver, SyncSender};

use rusty_core::Location;
use rusty_core::find::FindQuery;
use rusty_fs::find::{FindError, FoundBatch, FoundFile};
use rusty_fs::{CancelToken, JobKind, JobSlot, OwnerGuard, Pools, Priority, volume_of};

use super::super::{Group, Hit, Row};
use super::{ContentWalk, target};

pub(crate) const LIMIT: usize = 20;

pub(crate) enum PreviewEvent {
    Row(Box<Row>),
    Done(Option<String>),
}

pub(crate) struct Preview {
    pub receive: Receiver<PreviewEvent>,
    cancel: CancelToken,
    _owner: OwnerGuard,
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

impl Preview {
    pub(super) fn start(
        walk: ContentWalk,
        base: Location,
        query: FindQuery,
        cancel: CancelToken,
    ) -> Self {
        let queue = Pools::shared().queue(JobKind::Find);
        let owner = queue.owner();
        let slot = JobSlot {
            volume: volume_of(&base),
            priority: Priority::Visible,
            owner: owner.id(),
        };
        let (send, receive) = mpsc::sync_channel(LIMIT + 1);
        let signal = cancel.clone();
        queue.submit(
            slot,
            Box::new(move |_| {
                let mut delivery = Delivery {
                    send,
                    base,
                    count: 0,
                    archives: query.archives,
                };
                let base = delivery.base.clone();
                let result = walk(&base, &query, &signal, &mut |batch| {
                    delivery.batch(batch, &signal)
                });
                let error = match result {
                    Err(FindError::Cancelled) | Ok(_) => None,
                    Err(error) => Some(error.to_string()),
                };
                let _ = delivery.send.try_send(PreviewEvent::Done(error));
            }),
        );
        Self {
            receive,
            cancel,
            _owner: owner,
        }
    }
}

struct Delivery {
    send: SyncSender<PreviewEvent>,
    base: Location,
    count: usize,
    archives: bool,
}

impl Delivery {
    fn row(&self, relative: &str, hit: &rusty_core::find::LineHit) -> Option<Row> {
        let target = target::target(&self.base, relative, hit.line, self.archives)?;
        Some(Row::Hit(Hit {
            source: "content",
            group: Group::Files,
            title: format!("{} · {}:{}", hit.text, relative, hit.line).into(),
            detail: "".into(),
            score: 0,
            indices: Vec::new(),
            target,
            shortcut: None,
        }))
    }

    fn batch(&mut self, batch: FoundBatch, cancel: &CancelToken) -> ControlFlow<()> {
        for file in batch.files {
            self.file(file, cancel)?;
        }
        if self.count == LIMIT {
            ControlFlow::Break(())
        } else {
            ControlFlow::Continue(())
        }
    }

    fn file(&mut self, file: FoundFile, cancel: &CancelToken) -> ControlFlow<()> {
        let relative = file.relative.to_text();
        let hits = if file.hits.hits.is_empty() {
            vec![rusty_core::find::LineHit {
                line: 1,
                column: 0,
                text: relative.clone(),
            }]
        } else {
            file.hits.hits
        };
        for hit in hits {
            self.send_hit((&relative, hit), cancel)?;
        }
        ControlFlow::Continue(())
    }

    fn send_hit(
        &mut self,
        (relative, hit): (&str, rusty_core::find::LineHit),
        cancel: &CancelToken,
    ) -> ControlFlow<()> {
        if cancel.is_cancelled() || self.count == LIMIT {
            return ControlFlow::Break(());
        }
        let Some(row) = self.row(relative, &hit) else {
            return ControlFlow::Continue(());
        };
        if self
            .send
            .try_send(PreviewEvent::Row(Box::new(row)))
            .is_err()
        {
            return ControlFlow::Break(());
        }
        self.count += 1;
        ControlFlow::Continue(())
    }
}
