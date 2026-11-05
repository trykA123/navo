use std::io;
use std::path::PathBuf;
use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use navo_index::file::{self, Header};
use navo_index::ntfs::{NtfsError, NtfsIndex, SizeHint, UnresolvedChange, Volume};
use navo_index::pipe::CancelToken;

use super::answer::Shared;
use super::build_permit::{BUILDS, BuildPermit};
use super::config::{Drive, index_path};
use super::lifecycle::{
    SAVE_FAILURE, SaveClock, UNRESOLVED, UPDATE_FAILURE, begin_build, build_hint, clear_failures,
    report_unknown_sizes, restore_or_enumerate,
};
use super::rebuild::{Cause, Rebuilds, error_kind, file_error_kind};
use super::service_log::Log;
use super::storage::Storage;

pub(super) struct Worker {
    drive: Drive,
    path: PathBuf,
    shared: Shared,
    clock: SaveClock,
    started: Instant,
    rebuilds: Rebuilds,
}

impl Worker {
    pub fn new(drive: Drive, shared: Shared, storage: &Storage, log: Log) -> io::Result<Self> {
        Ok(Self {
            drive,
            shared,
            path: index_path(&storage.program_data, drive.letter)?,
            clock: SaveClock::default(),
            started: Instant::now(),
            rebuilds: Rebuilds::new(drive.letter, log),
        })
    }

    pub fn run(&mut self, cancel: &CancelToken) -> io::Result<()> {
        let mut volume = None;
        while !cancel.is_cancelled() {
            let result = self.tick(&mut volume, cancel);
            if let Err(NtfsError::Cancelled) = result {
                break;
            }
            if let Err(error) = result {
                let kind = error_kind(&error);
                self.log_line(format_args!("update error {}: {kind}", self.drive.letter));
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
            *volume = Some(self.open(cancel)?);
        }
        let Some(volume) = volume else {
            return Ok(());
        };
        if let Some(cause) = self.rebuilds.due(self.started.elapsed()) {
            return self.rebuild(volume, cause, cancel);
        }
        let followed = self.follow(volume);
        match self
            .rebuilds
            .after_follow(&self.shared, followed, self.started.elapsed())
        {
            Ok(None) => Ok(()),
            Ok(Some(cause)) => self.rebuild(volume, cause, cancel),
            Err(error) => {
                {
                    let mut state = self.shared.write().unwrap_or_else(PoisonError::into_inner);
                    state.index = None;
                    state.status.progress = Some(0);
                }
                self.clock.saved();
                Err(error)
            }
        }
    }

    fn open(&mut self, cancel: &CancelToken) -> Result<Volume, NtfsError> {
        let mut opened = Volume::open(self.drive.letter)?;
        self.drive.serial = opened.serial()?;
        let mut built = false;
        let mut permit = None;
        let index = restore_or_enumerate(file::load_with_ids(&self.path), self.drive, |reason| {
            built = true;
            let (index, held) = self.build(&mut opened, Cause::new(reason), cancel)?;
            permit = Some(held);
            Ok(index)
        })?;
        if !built {
            let entries = index.table.len();
            self.log_line(format_args!(
                "snapshot restored {}: entries={entries}",
                self.drive.letter
            ));
        }
        self.replace(index);
        drop(permit);
        Ok(opened)
    }

    /// Builds the new index while the old one keeps answering, then swaps them.
    fn rebuild(
        &mut self,
        volume: &mut Volume,
        cause: Cause,
        cancel: &CancelToken,
    ) -> Result<(), NtfsError> {
        let (index, permit) = self.build(volume, cause, cancel)?;
        self.replace(index);
        drop(permit);
        Ok(())
    }

    fn follow(&mut self, volume: &mut Volume) -> Result<Vec<UnresolvedChange>, NtfsError> {
        let position = self
            .shared
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .index
            .as_ref()
            .map(|index| index.position);
        let Some(position) = position else {
            return Ok(Vec::new());
        };
        self.shared
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .status
            .progress = Some(100);
        let batch = volume.read_journal(position)?;
        let records = batch.records.len();
        let changed = batch.next_usn != position.next_usn;
        let unresolved = super::batch::apply_batch(&self.shared, batch)?;
        if changed {
            self.clock.batch(self.started.elapsed(), records);
        }
        self.shared
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .status
            .progress = None;
        Ok(unresolved)
    }

    /// Sizes the next build from the index it replaces.
    fn rebuild_hint(&self) -> SizeHint {
        build_hint(
            self.shared
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .index
                .as_deref(),
        )
    }

    fn build(
        &mut self,
        volume: &mut Volume,
        cause: Cause,
        cancel: &CancelToken,
    ) -> Result<(NtfsIndex, BuildPermit<'static>), NtfsError> {
        self.rebuilds
            .started(&self.shared, cause, self.started.elapsed());
        // The index being replaced must never reach a snapshot, even when this build fails.
        self.clock.saved();
        let started = Instant::now();
        let total_files = volume
            .mft_records()
            .inspect_err(|error| tracing::warn!(%error, "reading the MFT record count"))
            .ok();
        begin_build(
            &mut self
                .shared
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .status,
            total_files,
        );
        let hint = self.rebuild_hint();
        self.discard_snapshot()?;
        self.log_line(format_args!("build queued {}:", self.drive.letter));
        let permit = BUILDS.acquire(cancel)?;
        self.log_line(format_args!(
            "build start {}: hint_entries={}",
            self.drive.letter, hint.entries
        ));
        let enumerated = volume.enumerate_sized(hint, cancel)?;
        self.log_line(format_args!(
            "build done {}: entries={} blocked_peak={} ms={}",
            self.drive.letter,
            enumerated.index.table.len(),
            enumerated.blocked_peak,
            started.elapsed().as_millis()
        ));
        report_unknown_sizes(
            &mut self
                .shared
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .status
                .errors,
            enumerated.sizes_unknown,
        );
        if !enumerated.unresolved.is_empty() {
            self.error(format!("{UNRESOLVED}file references"));
        }
        let entries = enumerated.index.table.len();
        self.rebuilds
            .finished(&self.shared, entries, started.elapsed());
        Ok((enumerated.index, permit))
    }

    fn discard_snapshot(&self) -> Result<(), NtfsError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {
                self.log_line(format_args!("snapshot discarded {}:", self.drive.letter));
                Ok(())
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(NtfsError::Io {
                operation: "discard snapshot",
                source,
            }),
        }
    }

    fn replace(&mut self, index: NtfsIndex) {
        let mut state = self.shared.write().unwrap_or_else(PoisonError::into_inner);
        state.status.progress = None;
        let old = state.index.replace(Arc::new(index));
        drop(old);
        // Trim only after the old index is gone, so the copy does not double the rebuild peak.
        if let Some(index) = state.index.as_mut().and_then(Arc::get_mut) {
            index.table.trim_slack();
        }
        self.clock.changed(self.started.elapsed());
    }

    fn save(&mut self, stopping: bool) -> io::Result<()> {
        // A deferred rebuild means the index misses changes; the older snapshot replays them.
        if self.rebuilds.is_deferred() {
            return Ok(());
        }
        let Some(index) = super::batch::published(&self.shared) else {
            return Ok(());
        };
        let snapshot = index.snapshot();
        if !self
            .clock
            .due(self.started.elapsed(), snapshot.as_ref(), stopping)
        {
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
        let entries = table.len();
        let saved = self
            .clock
            .save(&self.path, (header, table, ids), self.started.elapsed());
        let letter = self.drive.letter;
        match &saved {
            Ok(()) => self.log_line(format_args!("snapshot saved {letter}: entries={entries}")),
            Err(error) => {
                let kind = file_error_kind(error);
                self.log_line(format_args!("snapshot save failed {letter}: {kind}"));
            }
        }
        saved.map_err(io::Error::other)
    }

    fn log_line(&self, text: std::fmt::Arguments<'_>) {
        self.rebuilds.log().line(text);
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

pub(super) fn record_error(shared: &Shared, message: String) {
    let mut state = shared.write().unwrap_or_else(PoisonError::into_inner);
    if !state.status.errors.contains(&message) {
        state.status.errors.truncate(3);
        state.status.errors.push(message);
    }
}
