use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError, RwLock, RwLockReadGuard, TryLockError};
use std::time::{Duration, Instant};

use navo_index::ntfs::NtfsIndex;
use navo_index::pipe::{self, CancelToken, Hit, Reply, Request, VolumeBuilds, VolumeStatus};

use super::answer_stats::BUSY_REPLIES;
use super::search::SearchIndex;

const READ_WAIT: Duration = Duration::from_millis(20);
/// All volumes' read waits end by then: the client drops a reply after `QUERY_TIMEOUT` (200 ms).
const ANSWER_BUDGET: Duration = Duration::from_millis(120);
const READ_POLL: Duration = Duration::from_millis(1);
const UPDATING: u8 = 100;

/// How long one answer waits for read locks: per volume and for all volumes together.
#[derive(Clone, Copy)]
struct Waits {
    read: Duration,
    budget: Duration,
}

const WAITS: Waits = Waits {
    read: READ_WAIT,
    budget: ANSWER_BUDGET,
};

/// Follower counters for the heartbeat; service-internal, not on the wire.
#[derive(Default)]
pub(super) struct FollowStats {
    pub(super) errors: u64,
    pub(super) index_copies: u64,
    pub(super) lag_bytes: u64,
    pub(super) last_full_build: Option<Instant>,
}

#[derive(Default)]
pub(super) struct State {
    /// The published index; whoever changes or replaces it calls `search.forget_profile_ids()`.
    pub(super) index: Option<Arc<NtfsIndex>>,
    pub(super) status: VolumeStatus,
    pub(super) search: SearchIndex,
    pub(super) builds: VolumeBuilds,
    pub(super) follow: FollowStats,
    /// A successful journal batch skipped a cycle that still needs verification.
    pub(super) replay_inexact: bool,
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

/// Answers one pipe request; each volume waits at most `READ_WAIT` for its read lock, all of them
/// within `ANSWER_BUDGET`, and a volume that stays busy answers with its last status, marked updating.
pub(super) fn answer(volumes: &[SharedVolume], request: &Request, cancel: &CancelToken) -> Reply {
    answer_with(volumes, request, cancel, WAITS)
}

/// `answer` with injectable waits, so tests can outlast scheduler jitter on a loaded host.
fn answer_with(
    volumes: &[SharedVolume],
    request: &Request,
    cancel: &CancelToken,
    waits: Waits,
) -> Reply {
    if let Some(paths) = request.folder_paths() {
        return super::totals::answer(volumes, paths);
    }
    if request.wants_memory() {
        return super::memory::answer(volumes);
    }
    let mut reply = Reply::default();
    let overall = Instant::now() + waits.budget;
    for volume in volumes {
        if cancel.is_cancelled() {
            break;
        }
        // Each volume gets its own wait, so a slow first volume cannot starve the next one.
        let deadline = (Instant::now() + waits.read).min(overall);
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
        if !request.wants_status() {
            hits.extend(state.search.hits(&index.table, request, cancel));
        }
    }
    status.clamp_total();
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
            Err(TryLockError::WouldBlock) => {
                BUSY_REPLIES.fetch_add(1, Ordering::Relaxed);
                return None;
            }
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

    fn building_volume() -> SharedVolume {
        SharedVolume::new(State {
            status: VolumeStatus {
                root: "C:".into(),
                ..VolumeStatus::default()
            },
            ..State::default()
        })
    }

    fn status_of(volume: &SharedVolume) -> VolumeStatus {
        let request = Request {
            terms: vec![pipe::QueryTerm::StatusV1],
        };
        let reply = answer(std::slice::from_ref(volume), &request, &CancelToken::new());
        assert!(reply.hits.is_empty(), "a status request never searches");
        reply.status.volumes[0].clone()
    }

    /// The release lands `READ_WAIT + 10 ms` after the first volume gives up, as when a loaded
    /// host preempts this thread; a generous injected wait keeps that inside the second volume's
    /// own wait, while a wait shared with the first volume would already be spent.
    #[test]
    fn a_slow_first_volume_leaves_the_second_its_own_read_wait() {
        let waits = Waits {
            read: Duration::from_millis(250),
            budget: Duration::from_secs(1),
        };
        let volumes = [building_volume(), building_volume()];
        let _first = volumes[0].shared.write().unwrap();
        let second = volumes[1].shared.write().unwrap();
        let gave_up = BUSY_REPLIES.load(Ordering::Relaxed);
        let reply = std::thread::scope(|scope| {
            let asking = scope
                .spawn(|| answer_with(&volumes, &Request::default(), &CancelToken::new(), waits));
            while BUSY_REPLIES.load(Ordering::Relaxed) == gave_up {
                std::thread::yield_now();
            }
            // The second lock frees after the first volume's wait ran out.
            std::thread::sleep(READ_WAIT + Duration::from_millis(10));
            drop(second);
            asking.join().unwrap()
        });
        assert_eq!(reply.status.volumes[0].progress, Some(UPDATING));
        assert_eq!(
            reply.status.volumes[1].progress, None,
            "the second volume must get its own wait, not the first one's leftover"
        );
    }

    #[test]
    fn many_busy_volumes_answer_before_the_client_gives_up() {
        let volumes: Vec<_> = (0..12).map(|_| building_volume()).collect();
        let _writing: Vec<_> = volumes.iter().map(|v| v.shared.write().unwrap()).collect();
        let (reply, took) = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let started = Instant::now();
                    let reply = answer(&volumes, &Request::default(), &CancelToken::new());
                    (reply, started.elapsed())
                })
                .join()
                .unwrap()
        });
        let limit = pipe::QUERY_TIMEOUT - Duration::from_millis(40);
        assert!(took < limit, "answered after {took:?}");
        assert_eq!(reply.status.volumes.len(), volumes.len());
        assert!(
            reply
                .status
                .volumes
                .iter()
                .all(|status| status.progress == Some(UPDATING))
        );
    }

    #[test]
    fn the_total_is_unknown_until_a_build_starts_and_never_below_the_entries() {
        let volume = building_volume();
        assert_eq!(status_of(&volume).total_files, None);
        super::super::lifecycle::begin_build(&mut volume.shared.write().unwrap().status, Some(10));
        let building = status_of(&volume);
        assert_eq!(
            (building.progress, building.total_files),
            (Some(0), Some(10))
        );
        let index = navo_index::ntfs::test_support::MftFixture::new(40, 7)
            .read()
            .index;
        let entries = index.table.len() as u64;
        volume.shared.write().unwrap().index = Some(Arc::new(index));
        let built = status_of(&volume);
        assert_eq!((built.entries, built.total_files), (entries, Some(entries)));
        assert!(entries > 10, "the fixture must outgrow the estimate");
    }

    #[test]
    fn a_failed_count_leaves_the_total_unknown() {
        let volume = building_volume();
        let mut state = volume.shared.write().unwrap();
        state.status.total_files = Some(5);
        super::super::lifecycle::begin_build(&mut state.status, None);
        drop(state);
        assert_eq!(status_of(&volume).total_files, None);
    }
}
