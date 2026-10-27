use std::sync::{Arc, Mutex, PoisonError, RwLock, RwLockReadGuard, TryLockError};
use std::time::{Duration, Instant};

use rusty_index::ntfs::NtfsIndex;
use rusty_index::pipe::{self, CancelToken, Hit, Reply, Request, VolumeStatus};

use super::search::SearchIndex;

const READ_WAIT: Duration = Duration::from_millis(20);
const READ_POLL: Duration = Duration::from_millis(1);
const UPDATING: u8 = 100;

#[derive(Default)]
pub(super) struct State {
    pub(super) index: Option<NtfsIndex>,
    pub(super) status: VolumeStatus,
    pub(super) search: SearchIndex,
}

pub(super) type Shared = Arc<RwLock<State>>;

pub(super) struct SharedVolume {
    pub(super) shared: Shared,
    last: Mutex<VolumeStatus>,
}

impl SharedVolume {
    pub(super) fn new(state: State) -> Self {
        Self {
            last: Mutex::new(state.status.clone()),
            shared: Arc::new(RwLock::new(state)),
        }
    }
}

pub(super) fn answer(volumes: &[SharedVolume], request: &Request, cancel: &CancelToken) -> Reply {
    if let Some(paths) = request.folder_paths() {
        return super::totals::answer(volumes, paths);
    }
    if request.wants_memory() {
        return super::memory::answer(volumes);
    }
    let deadline = Instant::now() + READ_WAIT;
    let mut reply = Reply::default();
    for volume in volumes {
        if cancel.is_cancelled() {
            break;
        }
        let read = read_before(&volume.shared, deadline)
            .map(|state| search(&state, request, cancel, &mut reply.hits));
        let mut last = volume.last.lock().unwrap_or_else(PoisonError::into_inner);
        let status = match read {
            Some(status) => {
                last.clone_from(&status);
                status
            }
            None => VolumeStatus {
                progress: last.progress.or(Some(UPDATING)),
                ..last.clone()
            },
        };
        reply.status.volumes.push(status);
    }
    reply.hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
    });
    reply.hits.truncate(pipe::MAX_HITS);
    reply
}

fn search(
    state: &State,
    request: &Request,
    cancel: &CancelToken,
    hits: &mut Vec<Hit>,
) -> VolumeStatus {
    let mut status = state.status.clone();
    if let Some(index) = &state.index {
        status.entries = index.table.len() as u64;
        hits.extend(state.search.hits(&index.table, request, cancel));
    }
    status
}

pub(super) fn read_before<T>(
    lock: &RwLock<T>,
    deadline: Instant,
) -> Option<RwLockReadGuard<'_, T>> {
    loop {
        match lock.try_read() {
            Ok(guard) => return Some(guard),
            Err(TryLockError::Poisoned(poisoned)) => return Some(poisoned.into_inner()),
            Err(TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(READ_POLL);
            }
            Err(TryLockError::WouldBlock) => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume() -> SharedVolume {
        SharedVolume::new(State {
            status: VolumeStatus {
                root: "C:".into(),
                entries: 2_000_000,
                errors: vec!["updating volume index: busy".into()],
                ..VolumeStatus::default()
            },
            ..State::default()
        })
    }

    fn answer_while_writing(volume: &SharedVolume) -> (Reply, Duration) {
        let writing = volume.shared.write().unwrap();
        let answered = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let started = Instant::now();
                    let reply = answer(
                        std::slice::from_ref(volume),
                        &Request::default(),
                        &CancelToken::new(),
                    );
                    (reply, started.elapsed())
                })
                .join()
                .unwrap()
        });
        drop(writing);
        answered
    }

    #[test]
    fn a_query_during_a_long_write_answers_updating_with_the_last_status_within_50ms() {
        let (reply, took) = answer_while_writing(&volume());
        assert!(took < Duration::from_millis(50), "answered after {took:?}");
        assert!(reply.hits.is_empty());
        assert_eq!(
            reply.status.volumes,
            vec![VolumeStatus {
                progress: Some(UPDATING),
                ..volume().last.into_inner().unwrap()
            }]
        );
    }

    #[test]
    fn an_updating_answer_repeats_the_status_of_the_last_read() {
        let volume = volume();
        volume.shared.write().unwrap().status.entries = 7;
        answer(
            std::slice::from_ref(&volume),
            &Request::default(),
            &CancelToken::new(),
        );
        let (reply, _) = answer_while_writing(&volume);
        assert_eq!(reply.status.volumes[0].entries, 7);
    }
}
