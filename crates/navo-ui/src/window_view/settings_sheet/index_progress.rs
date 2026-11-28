use std::time::Duration;

use gpui_kit::{AsyncApp, Task, WeakEntity};
use navo_index::pipe::{Query, QueryTerm, Request};

use super::{Section, section_of};
use crate::browser::{FolderTotalsQuery, TotalsState};
use crate::command_center::index_source::{PipeAnswer, PipeQueries};
use crate::command_center::{Cancel, IndexMode, IndexOptions};
use crate::prelude::*;
use crate::settings::IndexProgress;
use crate::window_view::NavoWindow;

const EVERY: Duration = Duration::from_secs(1);

/// Asks the service for its status, memory and each volume root's folder totals (the file/folder
/// split). `None` means it was busy and the last view stands.
pub(super) fn fetch(
    pipe: &PipeQueries,
    totals: &FolderTotalsQuery,
    options: &IndexOptions,
) -> Option<IndexProgress> {
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
    let mut status = None;
    for terms in [
        vec![QueryTerm::StatusV2],
        vec![QueryTerm::StatusV1],
        Vec::new(),
    ] {
        match ask(terms, 1) {
            PipeAnswer::Replied(reply) => {
                status = Some(reply.status);
                break;
            }
            PipeAnswer::Absent => return Some(IndexProgress::default()),
            PipeAnswer::Silent => {}
        }
    }
    let status = status?;
    let memory = match ask(vec![QueryTerm::MemoryV1], 2) {
        PipeAnswer::Replied(reply) => reply.memory,
        PipeAnswer::Silent | PipeAnswer::Absent => None,
    };
    let progress = IndexProgress::of(Some(&status), memory.as_ref());
    let roots = root_totals(totals, &progress.roots());
    Some(progress.with_totals(&roots))
}

/// Folder totals for the volume roots, one per root in order; a root that is no valid location
/// stays unavailable on its own and the others are still asked.
fn root_totals(totals: &FolderTotalsQuery, roots: &[String]) -> Vec<TotalsState> {
    let mut answers = vec![TotalsState::Unavailable; roots.len()];
    let (slots, locations): (Vec<usize>, Vec<navo_core::Location>) = roots
        .iter()
        .enumerate()
        .filter_map(|(slot, root)| Some((slot, navo_core::Location::new(root).ok()?)))
        .unzip();
    if locations.is_empty() {
        return answers;
    }
    let found = totals.query_many(&locations, &navo_fs::CancelToken::new());
    for (slot, answer) in slots.into_iter().zip(found) {
        answers[slot] = answer;
    }
    answers
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
        let Ok((pipe, totals, options)) = this.read_with(cx, |view, _| {
            (
                view.settings.search.index.pipe.clone(),
                view.settings.search.folder_totals.clone(),
                IndexOptions::of(&view.values),
            )
        }) else {
            return false;
        };
        let found = cx
            .background_spawn(async move { fetch(&pipe, &totals, &options) })
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
    use crate::browser::no_folder_totals;
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
                    total_files: None,
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
                builds: None,
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
        let view = fetch(&queries, &no_folder_totals(), &options()).unwrap();
        assert_eq!(view.state(), IndexState::Building);
        assert_eq!((view.entries(), view.memory), (1234, Some(2048)));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn root_totals_give_each_volume_its_files_and_folders() {
        let (queries, _) = pipe(Box::new(|query| {
            if query.request.terms.contains(&QueryTerm::MemoryV1) {
                Ok(memory(1))
            } else {
                Ok(building())
            }
        }));
        let asked = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = asked.clone();
        let totals: FolderTotalsQuery = Arc::new(move |root: &navo_core::Location| {
            seen.lock().ok()?.push(root.as_str().to_owned());
            Some(navo_index::pipe::FolderTotals {
                files: 1000,
                folders: 233,
                complete: true,
                ..Default::default()
            })
        });
        let view = fetch(&queries, &totals, &options()).unwrap();
        let split = view.split().unwrap();
        assert_eq!((split.files, split.folders), (1000, 233));
        assert_eq!(asked.lock().unwrap().len(), 1);
    }

    #[test]
    fn an_invalid_root_is_unavailable_alone_and_the_others_keep_their_totals() {
        let totals: FolderTotalsQuery = Arc::new(|root: &navo_core::Location| {
            Some(navo_index::pipe::FolderTotals {
                files: u32::try_from(root.as_str().len()).ok()?,
                complete: true,
                ..Default::default()
            })
        });
        let roots = ["C:\\".to_owned(), "D\0:\\".to_owned(), "EE:\\".to_owned()];
        let answers = root_totals(&totals, &roots);
        let files = |answer: &TotalsState| match answer {
            TotalsState::Ready(found) => Some(found.files),
            TotalsState::Busy | TotalsState::Unavailable => None,
        };
        let files: Vec<_> = answers.iter().map(files).collect();
        assert_eq!(files, [Some(3), None, Some(4)]);
    }

    #[test]
    fn a_missing_service_is_not_running() {
        let (queries, _) = pipe(Box::new(|_| Err(PipeError::NotAvailable)));
        let view = fetch(&queries, &no_folder_totals(), &options()).unwrap();
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
        let view = fetch(&queries, &no_folder_totals(), &options()).unwrap();
        assert_eq!((view.state(), view.memory), (IndexState::Building, None));
    }

    #[test]
    fn an_old_service_that_drops_the_status_v2_request_falls_back_to_status_v1() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = seen.clone();
        let (queries, _) = pipe(Box::new(move |query| {
            recorded.lock().unwrap().push(query.request.terms.clone());
            if query.request.terms == vec![QueryTerm::StatusV2] {
                Err(PipeError::Io(std::io::ErrorKind::BrokenPipe.into()))
            } else if query.request.terms == vec![QueryTerm::StatusV1] {
                let mut reply = building();
                reply.status.volumes[0].total_files = Some(5000);
                Ok(reply)
            } else if query.request.terms == vec![QueryTerm::MemoryV1] {
                Ok(memory(4096))
            } else {
                panic!("unexpected fallback query: {:?}", query.request.terms);
            }
        }));
        let view = fetch(&queries, &no_folder_totals(), &options()).unwrap();
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                vec![QueryTerm::StatusV2],
                vec![QueryTerm::StatusV1],
                vec![QueryTerm::MemoryV1],
            ]
        );
        assert_eq!(
            (view.state(), view.entries(), view.memory),
            (IndexState::Building, 1234, Some(4096))
        );
    }

    #[test]
    fn an_old_service_that_drops_both_status_v2_and_v1_falls_back_to_plain_query() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = seen.clone();
        let (queries, _) = pipe(Box::new(move |query| {
            recorded.lock().unwrap().push(query.request.terms.clone());
            if query.request.terms == vec![QueryTerm::StatusV2]
                || query.request.terms == vec![QueryTerm::StatusV1]
            {
                Err(PipeError::Io(std::io::ErrorKind::BrokenPipe.into()))
            } else if query.request.terms == vec![QueryTerm::MemoryV1] {
                Ok(memory(4096))
            } else {
                Ok(building())
            }
        }));
        let view = fetch(&queries, &no_folder_totals(), &options()).unwrap();
        assert_eq!(
            *seen.lock().unwrap(),
            vec![
                vec![QueryTerm::StatusV2],
                vec![QueryTerm::StatusV1],
                Vec::new(),
                vec![QueryTerm::MemoryV1],
            ]
        );
        assert_eq!(
            (view.state(), view.memory),
            (IndexState::Building, Some(4096))
        );
    }

    #[test]
    fn a_silent_answer_keeps_the_last_view() {
        let (queries, _) = pipe(Box::new(|_| {
            Err(PipeError::Io(std::io::ErrorKind::TimedOut.into()))
        }));
        assert!(fetch(&queries, &no_folder_totals(), &options()).is_none());
    }

    #[test]
    fn walk_and_off_modes_never_touch_the_pipe() {
        let (queries, calls) = pipe(Box::new(|_| Ok(Reply::default())));
        for mode in [IndexMode::Walk, IndexMode::Off] {
            let options = IndexOptions { mode, ..options() };
            let view = fetch(&queries, &no_folder_totals(), &options).unwrap();
            assert_eq!(view.state(), IndexState::NotRunning);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}
