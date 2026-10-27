use std::cell::RefCell;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rusty_index::file::{self, Header};
use rusty_index::ntfs::{NtfsError, NtfsIndex, Volume};
use rusty_index::pipe::{CancelToken, Server, VolumeStatus};

use super::answer::{Shared, SharedVolume, State, answer};
use super::config::{Drive, index_path, select_volumes};
use super::lifecycle::{
    SAVE_FAILURE, SaveClock, UNRESOLVED, UPDATE_FAILURE, clear_failures, guard_worker,
    restore_or_enumerate,
};
use super::policy::ServiceError;
use super::search::SearchIndex;
use super::storage::Storage;

pub(super) fn run(
    cancel: &CancelToken,
    ready: impl FnOnce() -> io::Result<()>,
) -> Result<(), ServiceError> {
    let storage = Storage::open(false).map_err(ServiceError::Storage)?;
    let config = storage.read_config().map_err(ServiceError::Configuration)?;
    let available = super::volumes::discover().map_err(ServiceError::Configuration)?;
    let drives = select_volumes(&config.search, &available).map_err(ServiceError::Configuration)?;
    let volumes = initial_volumes(&drives, &config.search);
    std::thread::scope(|scope| {
        let _stop = StopOnDrop(cancel);
        let mut workers = Vec::new();
        for (drive, volume) in drives.iter().zip(&volumes) {
            let mut worker = Worker::new(*drive, Arc::clone(&volume.shared), &storage)
                .map_err(ServiceError::Storage)?;
            let shared = Arc::clone(&volume.shared);
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
        let result = serve_pipe(&config.sid, &volumes, cancel);
        cancel.cancel();
        result
            .and(join_workers(workers))
            .map_err(ServiceError::Unexpected)
    })
}

fn serve_pipe(sid: &str, volumes: &[SharedVolume], cancel: &CancelToken) -> io::Result<()> {
    let server = super::bind_retry::bind_with_retry(
        cancel,
        || Server::bind(sid),
        |message| {
            tracing::error!("{message}");
            for volume in volumes {
                record_error(&volume.shared, message.clone());
            }
        },
        |delay| super::bind_retry::wait_for_retry(cancel, delay),
    );
    let Some(mut server) = server else {
        return Ok(());
    };
    server
        .serve(
            &|request, query_cancel| answer(volumes, request, query_cancel),
            cancel,
        )
        .map_err(io::Error::other)
}

fn initial_volumes(drives: &[Drive], search: &rusty_store::Search) -> Vec<SharedVolume> {
    drives
        .iter()
        .map(|drive| {
            SharedVolume::new(State {
                status: VolumeStatus {
                    root: format!("{}:", drive.letter),
                    progress: Some(0),
                    ..Default::default()
                },
                search: SearchIndex::new(search),
                ..Default::default()
            })
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
                self.error(format!("{UPDATE_FAILURE}{error}"));
                volume = None;
            } else {
                self.recover(UPDATE_FAILURE);
            }
            match self.save(false) {
                Ok(()) => self.recover(SAVE_FAILURE),
                Err(error) => self.error(format!("{SAVE_FAILURE}{error}")),
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
        let mut state = self.shared.write().unwrap_or_else(PoisonError::into_inner);
        if let Some(index) = state.index.as_mut() {
            ensure_resolved(index.apply_batch(batch)?)?;
        }
        if changed {
            self.clock.changed(self.started.elapsed());
        }
        Ok(())
    }

    fn enumerate(&self, volume: &mut Volume, cancel: &CancelToken) -> Result<NtfsIndex, NtfsError> {
        {
            let mut state = self.shared.write().unwrap_or_else(PoisonError::into_inner);
            state.status.progress = Some(0);
            clear_failures(&mut state.status.errors, UNRESOLVED);
        }
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
            self.error(format!("{UNRESOLVED}file references"));
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

    fn recover(&self, prefix: &str) {
        let mut state = self.shared.write().unwrap_or_else(PoisonError::into_inner);
        if !state.status.errors.is_empty() {
            clear_failures(&mut state.status.errors, prefix);
        }
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
