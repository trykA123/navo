use std::cell::RefCell;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, PoisonError, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusty_index::file::{self, Header};
use rusty_index::ntfs::{NtfsError, NtfsIndex, Volume};
use rusty_index::pipe::{self, CancelToken, Hit, Reply, Request, Server, VolumeStatus};
use rusty_index::search;

use super::config::{Drive, index_path, select_volumes};
use super::lifecycle::{SaveClock, guard_worker, restore_or_enumerate};
use super::policy::ServiceError;
use super::storage::Storage;

#[derive(Default)]
struct State {
    index: Option<NtfsIndex>,
    status: VolumeStatus,
}

type Shared = Arc<RwLock<State>>;

pub(super) fn run(
    cancel: &CancelToken,
    ready: impl FnOnce() -> io::Result<()>,
) -> Result<(), ServiceError> {
    let storage = Storage::open(false).map_err(ServiceError::Storage)?;
    let config = storage.read_config().map_err(ServiceError::Configuration)?;
    let available = super::volumes::discover().map_err(ServiceError::Configuration)?;
    let drives = select_volumes(&config.search, &available).map_err(ServiceError::Configuration)?;
    let shared = initial_states(&drives);
    std::thread::scope(|scope| {
        let _stop = StopOnDrop(cancel);
        let mut workers = Vec::new();
        for (drive, shared) in drives.iter().zip(&shared) {
            let mut worker =
                Worker::new(*drive, Arc::clone(shared), &storage).map_err(ServiceError::Storage)?;
            let shared = Arc::clone(shared);
            workers.push(
                std::thread::Builder::new()
                    .name("ntfs-index".into())
                    .spawn_scoped(scope, move || {
                        guard_worker(
                            cancel,
                            |message| record_error(&shared, message),
                            || worker.run(cancel),
                        )
                    })
                    .map_err(ServiceError::Unexpected)?,
            );
        }
        ready().map_err(ServiceError::Unexpected)?;
        let result = serve_pipe(&config.sid, &shared, cancel);
        cancel.cancel();
        result
            .and(join_workers(workers))
            .map_err(ServiceError::Unexpected)
    })
}

fn serve_pipe(sid: &str, shared: &[Shared], cancel: &CancelToken) -> io::Result<()> {
    let server = super::bind_retry::bind_with_retry(
        cancel,
        || Server::bind(sid),
        |message| {
            tracing::error!("{message}");
            for state in shared {
                record_error(state, message.clone());
            }
        },
        |delay| super::bind_retry::wait_for_retry(cancel, delay),
    );
    let Some(mut server) = server else {
        return Ok(());
    };
    server
        .serve(
            &|request, query_cancel| answer(shared, request, query_cancel),
            cancel,
        )
        .map_err(io::Error::other)
}

fn initial_states(drives: &[Drive]) -> Vec<Shared> {
    drives
        .iter()
        .map(|drive| {
            Arc::new(RwLock::new(State {
                status: VolumeStatus {
                    root: format!("{}:", drive.letter),
                    progress: Some(0),
                    ..Default::default()
                },
                ..Default::default()
            }))
        })
        .collect()
}

fn join_workers(workers: Vec<std::thread::ScopedJoinHandle<'_, io::Result<()>>>) -> io::Result<()> {
    let mut result = Ok(());
    for worker in workers {
        while !worker.is_finished() {
            result = result.and(super::scm::stopping());
            std::thread::park_timeout(Duration::from_millis(500));
        }
        result = result.and(
            worker
                .join()
                .map_err(|_| io::Error::other("index worker failed"))
                .and_then(|result| result),
        );
    }
    result
}

struct StopOnDrop<'a>(&'a CancelToken);

impl Drop for StopOnDrop<'_> {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

struct Worker {
    drive: Drive,
    path: PathBuf,
    shared: Shared,
    clock: SaveClock,
    started: Instant,
}

impl Worker {
    fn new(drive: Drive, shared: Shared, storage: &Storage) -> io::Result<Self> {
        Ok(Self {
            drive,
            shared,
            path: index_path(&storage.program_data, drive.letter)?,
            clock: SaveClock::default(),
            started: Instant::now(),
        })
    }

    fn run(&mut self, cancel: &CancelToken) -> io::Result<()> {
        let mut volume = None;
        while !cancel.is_cancelled() {
            let result = self.tick(&mut volume, cancel);
            if let Err(NtfsError::Cancelled) = result {
                break;
            }
            if let Err(error) = result {
                self.error(format!("updating volume index: {error}"));
                volume = None;
            }
            if let Err(error) = self.save(false) {
                self.error(format!("saving volume index: {error}"));
            }
            std::thread::park_timeout(Duration::from_millis(500));
        }
        if let Err(error) = self.save(true) {
            self.error(format!("saving volume index on stop: {error}"));
        }
        Ok(())
    }

    fn tick(&mut self, volume: &mut Option<Volume>, cancel: &CancelToken) -> Result<(), NtfsError> {
        if volume.is_none() {
            let opened = Volume::open(self.drive.letter)?;
            self.drive.serial = opened.serial()?;
            let opened = RefCell::new(opened);
            let index = restore_or_enumerate(
                file::load_with_ids(&self.path),
                self.drive,
                |index| resume(&mut opened.borrow_mut(), index),
                || self.enumerate(&mut opened.borrow_mut(), cancel),
            )?;
            self.replace(index);
            *volume = Some(opened.into_inner());
        }
        let Some(volume) = volume else {
            return Ok(());
        };
        if let Err(error) = self.follow(volume) {
            self.shared
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .index = None;
            self.clock.saved();
            if matches!(error, NtfsError::Reenumerate) {
                let index = self.enumerate(volume, cancel)?;
                self.replace(index);
                return Ok(());
            }
            return Err(error);
        }
        Ok(())
    }

    fn follow(&mut self, volume: &mut Volume) -> Result<(), NtfsError> {
        let position = self
            .shared
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .index
            .as_ref()
            .map(|index| index.position);
        let Some(position) = position else {
            return Ok(());
        };
        let batch = volume.read_journal(position)?;
        let changed = batch.next_usn != position.next_usn;
        let result = self
            .shared
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .index
            .as_mut()
            .map(|index| index.apply_batch(batch));
        if let Some(result) = result {
            ensure_resolved(result?)?;
        }
        if changed {
            self.clock.changed(self.started.elapsed());
        }
        Ok(())
    }

    fn enumerate(&self, volume: &mut Volume, cancel: &CancelToken) -> Result<NtfsIndex, NtfsError> {
        self.shared
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .status
            .progress = Some(0);
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(NtfsError::Io {
                    operation: "discard snapshot",
                    source,
                });
            }
        }
        let enumerated = volume.enumerate(cancel)?;
        if !enumerated.unresolved.is_empty() {
            self.error("enumeration has unresolved file references".into());
        }
        Ok(enumerated.index)
    }

    fn replace(&mut self, index: NtfsIndex) {
        let mut state = self.shared.write().unwrap_or_else(PoisonError::into_inner);
        state.status.progress = None;
        state.index = Some(index);
        self.clock.changed(self.started.elapsed());
    }

    fn save(&mut self, stopping: bool) -> io::Result<()> {
        let state = self.shared.read().unwrap_or_else(PoisonError::into_inner);
        let Some(index) = &state.index else {
            return Ok(());
        };
        let snapshot = index.snapshot();
        if !self.clock.due(self.started.elapsed(), snapshot, stopping) {
            return Ok(());
        }
        let Some((table, ids, position)) = snapshot else {
            return Ok(());
        };
        let header = Header {
            volume_serial: self.drive.serial,
            journal_id: position.journal_id,
            next_usn: position.next_usn,
            root: format!("{}:", self.drive.letter),
            built_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
        };
        if let Err(error) = file::save_with_ids(&self.path, &header, table, ids) {
            self.clock.failed(self.started.elapsed());
            return Err(io::Error::other(error));
        }
        self.clock.saved();
        Ok(())
    }

    fn error(&self, message: String) {
        record_error(&self.shared, message);
    }
}

fn record_error(shared: &Shared, message: String) {
    let mut state = shared.write().unwrap_or_else(PoisonError::into_inner);
    if !state.status.errors.contains(&message) {
        state.status.errors.truncate(3);
        state.status.errors.push(message);
    }
}

fn resume(volume: &mut Volume, index: &mut NtfsIndex) -> Result<(), NtfsError> {
    let batch = volume.read_journal(index.position)?;
    ensure_resolved(index.apply_batch(batch)?)
}

fn ensure_resolved(changes: Vec<rusty_index::ntfs::UnresolvedChange>) -> Result<(), NtfsError> {
    if changes.is_empty() {
        Ok(())
    } else {
        Err(NtfsError::Reenumerate)
    }
}

fn answer(volumes: &[Shared], request: &Request, cancel: &CancelToken) -> Reply {
    let mut reply = Reply::default();
    for shared in volumes {
        if cancel.is_cancelled() {
            break;
        }
        let state = shared.read().unwrap_or_else(PoisonError::into_inner);
        let mut status = state.status.clone();
        if let Some(index) = &state.index {
            status.entries = index.table.len() as u64;
            reply.hits.extend(
                search::search(&index.table, &request.terms(), pipe::MAX_HITS, &|| {
                    cancel.is_cancelled()
                })
                .into_iter()
                .filter_map(|hit| {
                    Some(Hit {
                        path: index.table.path_with(hit.id, '\\')?,
                        flags: index.table.flags(hit.id)?.bits(),
                        score: hit.score,
                        indices: hit.indices,
                    })
                }),
            );
        }
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
