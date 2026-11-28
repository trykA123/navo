use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, PoisonError, RwLock, RwLockReadGuard, TryLockError};
use std::time::{Duration, Instant};

use navo_index::ntfs::NtfsIndex;
use navo_index::pipe::{
    self, CancelToken, Hit, Reply, Request, VolumeBuilds, VolumeDetail, VolumeState, VolumeStatus,
};

use super::answer_stats::BUSY_REPLIES;
use super::search::SearchIndex;

const READ_WAIT: Duration = Duration::from_millis(20);
/// All volumes' read waits end by then: the client drops a reply after `QUERY_TIMEOUT` (200 ms).
const ANSWER_BUDGET: Duration = Duration::from_millis(120);
const READ_POLL: Duration = Duration::from_millis(1);
const UPDATING: u8 = 100;
/// Journal lag above 1 MiB marks a volume as `Behind` in `StatusV2`.
const LAG_BEHIND_BYTES: u64 = 1024 * 1024;

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
    /// Deadline of a deferred full rebuild, if one is waiting for the rebuild window.
    pub(super) rebuild_due_at: Option<Instant>,
    /// A successful journal batch skipped a cycle that still needs verification.
    pub(super) replay_inexact: bool,
}

pub(super) type Shared = Arc<RwLock<State>>;

pub(super) struct SharedVolume {
    pub(super) shared: Shared,
    last: Mutex<VolumeStatus>,
    last_detail: Mutex<VolumeDetail>,
}

impl SharedVolume {
    pub(super) fn new(state: State) -> Self {
        let last = state.status.clone();
        let last_detail = volume_detail(&state, &last, Instant::now());
        Self {
            last: Mutex::new(last),
            last_detail: Mutex::new(last_detail),
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
    let wants_v2 = request.wants_status_v2();
    let mut details = wants_v2.then(|| Vec::with_capacity(volumes.len()));
    let now = Instant::now();
    let overall = now + waits.budget;
    for volume in volumes {
        if cancel.is_cancelled() {
            break;
        }
        // Each volume gets its own wait, so a slow first volume cannot starve the next one.
        let deadline = (Instant::now() + waits.read).min(overall);
        let read = read_before(&volume.shared, deadline).map(|state| {
            let status = search(&state, request, cancel, &mut reply.hits);
            let detail = volume_detail(&state, &status, now);
            (status, detail)
        });
        let mut last = volume.last.lock().unwrap_or_else(PoisonError::into_inner);
        let mut last_detail = volume
            .last_detail
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let (status, detail) = match read {
            Some((status, detail)) => {
                last.clone_from(&status);
                last_detail.clone_from(&detail);
                (status, wants_v2.then_some(detail))
            }
            None => {
                let status = VolumeStatus {
                    progress: last.progress.or(Some(UPDATING)),
                    ..last.clone()
                };
                let detail = wants_v2.then(|| busy_detail(&status, &last_detail));
                (status, detail)
            }
        };
        reply.status.volumes.push(status);
        if let (Some(list), Some(detail)) = (&mut details, detail) {
            list.push(detail);
        }
    }
    reply.status_detail = details;
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
        if !request.wants_status() && !request.wants_status_v2() {
            hits.extend(state.search.hits(&index.table, request, cancel));
        }
    }
    status.clamp_total();
    status
}

/// Builds the `StatusV2` per-volume detail from the live `State` and clamped `VolumeStatus`.
fn volume_detail(state: &State, status: &VolumeStatus, now: Instant) -> VolumeDetail {
    let rebuild_in_s = state
        .rebuild_due_at
        .map(|due| u32::try_from(due.saturating_duration_since(now).as_secs()).unwrap_or(u32::MAX));
    let volume_state = if status.has_blocking_error() {
        VolumeState::Stale
    } else if rebuild_in_s.is_some() {
        VolumeState::RebuildWaiting
    } else if matches!(status.progress, Some(p) if p < UPDATING) {
        VolumeState::Building
    } else if state.follow.lag_bytes > LAG_BEHIND_BYTES {
        VolumeState::Behind
    } else if status.progress.is_some() {
        VolumeState::Updating
    } else {
        VolumeState::Ready
    };
    VolumeDetail {
        state: volume_state,
        total_files: status.total_files,
        lag_bytes: state.follow.lag_bytes,
        rebuild_in_s,
        full_builds: state.builds.full_builds,
        last_full_build_age_s: state
            .follow
            .last_full_build
            .map(|at| now.saturating_duration_since(at).as_secs()),
    }
}

/// Marks a cached detail as `Updating` when the volume lock is held by a write, preserving higher states
/// and keeping `total_files` and state aligned with the cached `VolumeStatus`.
fn busy_detail(status: &VolumeStatus, last: &VolumeDetail) -> VolumeDetail {
    let mut detail = last.clone();
    detail.total_files = status.total_files;
    if status.has_blocking_error() {
        detail.state = VolumeState::Stale;
    } else if matches!(status.progress, Some(p) if p < UPDATING) {
        detail.state = VolumeState::Building;
    } else if matches!(detail.state, VolumeState::Ready | VolumeState::Behind) {
        detail.state = VolumeState::Updating;
    }
    detail
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

    fn status_v2_of(volume: &SharedVolume) -> pipe::VolumeDetail {
        let request = Request {
            terms: vec![pipe::QueryTerm::StatusV2],
        };
        let reply = answer(std::slice::from_ref(volume), &request, &CancelToken::new());
        assert!(reply.hits.is_empty(), "StatusV2 never searches");
        reply
            .status_detail
            .expect("StatusV2 must populate status_detail")
            .remove(0)
    }

    #[test]
    fn status_v2_maps_blocking_error_deferred_rebuild_and_lag() {
        let stale = SharedVolume::new(State {
            status: VolumeStatus {
                root: "C:".into(),
                errors: vec!["updating volume index: access denied".into()],
                ..VolumeStatus::default()
            },
            rebuild_due_at: Some(Instant::now() + Duration::from_secs(300)),
            follow: FollowStats {
                lag_bytes: 2 * 1024 * 1024,
                ..FollowStats::default()
            },
            ..State::default()
        });
        assert_eq!(status_v2_of(&stale).state, pipe::VolumeState::Stale);

        let waiting = SharedVolume::new(State {
            status: VolumeStatus {
                root: "D:".into(),
                errors: vec![format!("{}4", pipe::SIZES_UNKNOWN_PREFIX)],
                ..VolumeStatus::default()
            },
            rebuild_due_at: Some(Instant::now() + Duration::from_secs(360)),
            builds: VolumeBuilds {
                full_builds: 2,
                ..VolumeBuilds::default()
            },
            follow: FollowStats {
                last_full_build: Some(Instant::now() - Duration::from_secs(90)),
                ..FollowStats::default()
            },
            ..State::default()
        });
        let waiting_detail = status_v2_of(&waiting);
        assert_eq!(waiting_detail.state, pipe::VolumeState::RebuildWaiting);
        assert!(
            waiting_detail
                .rebuild_in_s
                .is_some_and(|s| s > 0 && s <= 360)
        );
        assert_eq!(waiting_detail.full_builds, 2);
        assert!(
            waiting_detail
                .last_full_build_age_s
                .is_some_and(|s| s >= 90)
        );

        let behind = SharedVolume::new(State {
            status: VolumeStatus {
                root: "E:".into(),
                ..VolumeStatus::default()
            },
            follow: FollowStats {
                lag_bytes: 1024 * 1024 + 1,
                ..FollowStats::default()
            },
            ..State::default()
        });
        let behind_detail = status_v2_of(&behind);
        assert_eq!(behind_detail.state, pipe::VolumeState::Behind);
        assert_eq!(behind_detail.lag_bytes, 1024 * 1024 + 1);
    }

    #[test]
    fn a_deferred_journal_reset_reports_building_once_it_starts() {
        use super::super::lifecycle::begin_build;
        use super::super::rebuild::{Cause, REBUILD_WINDOW, Rebuilds};
        use super::super::service_log::Log;
        use navo_index::ntfs::NtfsError;
        use navo_index::pipe::BuildReason;

        let volume = building_volume();
        let mut rebuilds = Rebuilds::new('C', Log::default());
        let first_at = Duration::from_secs(60);
        rebuilds.started(
            &volume.shared,
            Cause::new(BuildReason::SnapshotMissing),
            first_at,
        );
        rebuilds.finished(&volume.shared, 10, Duration::from_secs(1));

        let deferred = rebuilds.after_follow(
            &volume.shared,
            Err(NtfsError::Reenumerate("journal")),
            first_at + Duration::from_secs(60),
        );
        assert_eq!(deferred.unwrap(), None);
        assert_eq!(
            status_v2_of(&volume).state,
            pipe::VolumeState::RebuildWaiting
        );

        let due_at = first_at + REBUILD_WINDOW;
        let due = rebuilds
            .due(due_at)
            .expect("journal reset should be due after the rebuild window");
        assert_eq!(due.reason, BuildReason::JournalReset);
        rebuilds.started(&volume.shared, due, due_at);
        {
            let mut state = volume.shared.write().unwrap();
            begin_build(&mut state.status, Some(100));
            state.status.progress = Some(25);
        }

        let detail = status_v2_of(&volume);
        assert_eq!(detail.state, pipe::VolumeState::Building);
        assert_eq!(detail.rebuild_in_s, None);
    }

    fn status_v2_while_writing(volume: &SharedVolume) -> Reply {
        let writing = volume.shared.write().unwrap();
        let reply = std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    let request = Request {
                        terms: vec![pipe::QueryTerm::StatusV2],
                    };
                    answer(std::slice::from_ref(volume), &request, &CancelToken::new())
                })
                .join()
                .unwrap()
        });
        drop(writing);
        reply
    }

    #[test]
    fn busy_status_v2_preserves_total_files_and_state_after_status_v1_reads() {
        let volume = building_volume();
        {
            let mut state = volume.shared.write().unwrap();
            super::super::lifecycle::begin_build(&mut state.status, Some(500));
            state.status.progress = Some(25);
            state.builds.full_builds = 1;
        }
        // Only StatusV1 runs before the write lock is taken.
        let v1 = status_of(&volume);
        assert_eq!(v1.total_files, Some(500));

        let reply = status_v2_while_writing(&volume);
        let detail = reply.status_detail.expect("StatusV2 must populate detail");
        assert_eq!(detail[0].total_files, Some(500));
        assert_eq!(detail[0].state, pipe::VolumeState::Building);
        assert_eq!(detail[0].full_builds, 1);

        // Verify wire round-trip does not wipe total_files.
        let wire = pipe::Response::StatusV2 {
            generation: 1,
            status: reply.status,
            detail,
        };
        let pipe::Response::StatusV2 { status, detail, .. } =
            pipe::frames::decode(&pipe::frames::encode(&wire).unwrap()).unwrap()
        else {
            panic!("must decode as StatusV2");
        };
        assert_eq!(status.volumes[0].total_files, Some(500));
        assert_eq!(detail[0].total_files, Some(500));

        // Blocking error read via StatusV1 must also surface as Stale under a busy lock.
        {
            let mut state = volume.shared.write().unwrap();
            state.status.progress = None;
            state.status.errors = vec!["updating volume index: access denied".into()];
        }
        let _ = status_of(&volume);
        let stale_reply = status_v2_while_writing(&volume);
        let stale_detail = stale_reply
            .status_detail
            .expect("StatusV2 must populate detail");
        assert_eq!(stale_detail[0].state, pipe::VolumeState::Stale);
    }
}
