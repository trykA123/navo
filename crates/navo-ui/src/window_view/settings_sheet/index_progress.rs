use std::time::Duration;

use gpui_kit::{AsyncApp, Task, WeakEntity};
use navo_index::pipe::{Query, QueryTerm, Request};

use super::{Section, section_of};
use crate::command_center::index_source::{PipeAnswer, PipeQueries};
use crate::command_center::{Cancel, IndexMode, IndexOptions};
use crate::prelude::*;
use crate::settings::IndexProgress;
use crate::window_view::NavoWindow;

const EVERY: Duration = Duration::from_secs(1);

/// Asks the service for its status and memory. `None` means it was busy and the last view stands.
pub(super) fn fetch(pipe: &PipeQueries, options: &IndexOptions) -> Option<IndexProgress> {
    if !matches!(options.mode, IndexMode::Auto | IndexMode::Service) {
        return Some(IndexProgress::default());
    }
    let ask = |terms, generation| {
        let query = Query {
            request: Request { terms },
            generation,
        };
        pipe.query(options.clone(), query, &Cancel::new())
    };
    let status = match ask(Vec::new(), 1) {
        PipeAnswer::Replied(reply) => reply.status,
        PipeAnswer::Absent => return Some(IndexProgress::default()),
        PipeAnswer::Silent => return None,
    };
    let memory = match ask(vec![QueryTerm::MemoryV1], 2) {
        PipeAnswer::Replied(reply) => reply.memory,
        PipeAnswer::Silent | PipeAnswer::Absent => None,
    };
    Some(IndexProgress::of(Some(&status), memory.as_ref()))
}

impl NavoWindow {
    pub(super) fn start_index_progress(&mut self, cx: &mut Context<Self>) {
        if self.settings.index_poll.is_some() {
            return;
        }
        self.settings.index_poll = Some(cx.spawn(async move |this, cx| {
            while Self::poll_index_progress(&this, cx).await {
                cx.background_executor().timer(EVERY).await;
            }
        }));
    }

    async fn poll_index_progress(this: &WeakEntity<Self>, cx: &mut AsyncApp) -> bool {
        let Ok((pipe, options)) = this.read_with(cx, |view, _| {
            (
                view.settings.search.index.pipe.clone(),
                IndexOptions::of(&view.values),
            )
        }) else {
            return false;
        };
        let found = cx
            .background_spawn(async move { fetch(&pipe, &options) })
            .await;
        this.update(cx, |view, cx| view.receive_index_progress(found, cx))
            .unwrap_or(false)
    }

    pub(super) fn stop_index_progress(&mut self) {
        self.settings.index_poll = None;
    }

    fn receive_index_progress(
        &mut self,
        found: Option<IndexProgress>,
        cx: &mut Context<Self>,
    ) -> bool {
        let shown = self.settings.is_open && section_of(self.settings.row) == Section::Search;
        if !shown {
            self.settings.index_poll = None;
            return false;
        }
        if let Some(found) = found.filter(|found| *found != self.settings.index_progress) {
            self.settings.index_progress = found;
            cx.notify();
        }
        true
    }
}

pub(super) type Poll = Option<Task<()>>;

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use navo_index::pipe::{
        MemoryPart, MemoryReport, Part, PipeError, Reply, Status, VolumeStatus,
    };

    use super::*;
    use crate::settings::IndexState;

    fn options() -> IndexOptions {
        IndexOptions::of(&crate::settings::Values::default())
    }

    type Script = Box<dyn Fn(&Query) -> Result<Reply, PipeError> + Send + Sync>;

    fn pipe(script: Script) -> (PipeQueries, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let counted = calls.clone();
        let queries = PipeQueries::new(Arc::new(move |_, query, _| {
            counted.fetch_add(1, Ordering::SeqCst);
            script(query)
        }));
        (queries, calls)
    }

    fn building() -> Reply {
        Reply {
            status: Status {
                volumes: vec![VolumeStatus {
                    root: "C:".into(),
                    entries: 1234,
                    progress: Some(0),
                    errors: Vec::new(),
                }],
            },
            ..Reply::default()
        }
    }

    fn memory(bytes: u64) -> Reply {
        Reply {
            memory: Some(MemoryReport {
                parts: vec![MemoryPart {
                    part: Part::Process.code(),
                    bytes,
                    items: 0,
                }],
                busy: false,
                process_id: None,
                volumes: None,
            }),
            ..Reply::default()
        }
    }

    #[test]
    fn status_and_memory_replies_fill_the_view() {
        let (queries, calls) = pipe(Box::new(|query| {
            if query.request.terms.contains(&QueryTerm::MemoryV1) {
                Ok(memory(2048))
            } else {
                Ok(building())
            }
        }));
        let view = fetch(&queries, &options()).unwrap();
        assert_eq!(view.state(), IndexState::Building);
        assert_eq!((view.files(), view.memory), (1234, Some(2048)));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_missing_service_is_not_running() {
        let (queries, _) = pipe(Box::new(|_| Err(PipeError::NotAvailable)));
        let view = fetch(&queries, &options()).unwrap();
        assert_eq!(view.state(), IndexState::NotRunning);
    }

    #[test]
    fn an_old_service_that_drops_the_memory_request_still_shows_its_status() {
        let (queries, _) = pipe(Box::new(|query| {
            if query.request.terms.contains(&QueryTerm::MemoryV1) {
                Err(PipeError::Io(std::io::ErrorKind::BrokenPipe.into()))
            } else {
                Ok(building())
            }
        }));
        let view = fetch(&queries, &options()).unwrap();
        assert_eq!((view.state(), view.memory), (IndexState::Building, None));
    }

    #[test]
    fn a_silent_answer_keeps_the_last_view() {
        let (queries, _) = pipe(Box::new(|_| {
            Err(PipeError::Io(std::io::ErrorKind::TimedOut.into()))
        }));
        assert!(fetch(&queries, &options()).is_none());
    }

    #[test]
    fn walk_and_off_modes_never_touch_the_pipe() {
        let (queries, calls) = pipe(Box::new(|_| Ok(Reply::default())));
        for mode in [IndexMode::Walk, IndexMode::Off] {
            let options = IndexOptions { mode, ..options() };
            let view = fetch(&queries, &options).unwrap();
            assert_eq!(view.state(), IndexState::NotRunning);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
