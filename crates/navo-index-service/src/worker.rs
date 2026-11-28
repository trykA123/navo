//! Runs a volume's index updates and delegates snapshot persistence to the rebuild session.

use std::io;
use std::path::PathBuf;
use std::sync::PoisonError;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use navo_index::file;
use navo_index::ntfs::{Enumeration, NtfsError, NtfsIndex, Volume};
use navo_index::pipe::CancelToken;

use super::answer::Shared;
use super::build_permit::{BUILDS, BuildPermit};
use super::config::{Drive, index_path};
use super::follow_pace::{Outcome, Pace};
use super::journal_create::JournalCreate;
use super::lifecycle::{
    SAVE_FAILURE, SaveClock, UNRESOLVED, UPDATE_FAILURE, begin_build, clear_failures,
    report_unknown_sizes, restore_or_enumerate,
};
use super::rebuild::{Cause, Rebuilds, error_kind, file_error_kind};
use super::rebuild_session::{SaveInput, rebuild_hint, replace, save_snapshot};
use super::restore_log::restored_line;
use super::service_log::Log;
use super::storage::Storage;

#[path = "worker_follow.rs"]
mod follow_batch;

pub(super) struct Worker {
    drive: Drive,
    path: PathBuf,
    shared: Shared,
    clock: SaveClock,
    started: Instant,
    rebuilds: Rebuilds,
    pace: Pace,
    /// True when the last follow moved the journal cursor; reset at the start of every tick.
    moved: bool,
    /// True after a regular build failed, so a reopen must not keep the stale in-memory index.
    build_failed: bool,
    /// Creates this selected volume's missing USN journal once (IXJ).
    journal: JournalCreate,
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
            pace: Pace::default(),
            moved: false,
            build_failed: false,
            journal: JournalCreate::default(),
        })
    }

    pub fn run(&mut self, cancel: &CancelToken) -> io::Result<()> {
        let mut volume = None;
        while !cancel.is_cancelled() {
            let result = self.tick(&mut volume, cancel);
            if let Err(NtfsError::Cancelled) = result {
                break;
            }
            let outcome = match &result {
                Err(_) => Outcome::Failed,
                Ok(()) if self.moved => Outcome::Moved,
                Ok(()) => Outcome::Idle,
            };
            if let Err(error) = result {
                let kind = error_kind(&error);
                self.log_line(format_args!("update error {}: {kind}", self.drive.letter));
                self.error(format!("{UPDATE_FAILURE}{error}"));
                self.shared
                    .write()
                    .unwrap_or_else(PoisonError::into_inner)
                    .follow
                    .errors += 1;
                volume = None;
            } else {
                self.recover(UPDATE_FAILURE);
            }
            match self.save(false) {
                Ok(()) => self.recover(SAVE_FAILURE),
                Err(error) => self.error(format!("{SAVE_FAILURE}{error}")),
            }
            if cancel.is_cancelled() {
                break;
            }
            let follow_delay = self.pace.next(outcome);
            wait(
                cancel,
                self.rebuilds.take_retry_delay().unwrap_or(follow_delay),
            );
        }
        if let Err(error) = self.save(true) {
            self.error(format!("saving volume index on stop: {error}"));
        }
        Ok(())
    }

    fn tick(&mut self, volume: &mut Option<Volume>, cancel: &CancelToken) -> Result<(), NtfsError> {
        // A moving poll followed by an error or a skipped poll must not keep zero-delay ticks.
        self.moved = false;
        if volume.is_none() {
            *volume = Some(self.open(cancel)?);
        }
        let Some(volume) = volume else {
            return Ok(());
        };
        let now = Instant::now();
        if let Some(reason) = self.rebuilds.verification_due(now)
            && self.verify(volume, reason, cancel)?
        {
            return Ok(());
        }
        if let Some(cause) = self.rebuilds.due(self.started.elapsed()) {
            return self.rebuild(volume, cause, cancel);
        }
        if self.rebuilds.journal_lost() {
            return Ok(());
        }
        // A journal read error keeps the published index; `run` retries with backoff (D121).
        match self.follow(volume, cancel)? {
            None => Ok(()),
            Some(cause) => self.rebuild(volume, cause, cancel),
        }
    }

    /// Opens the volume; keeps the live index when the serial is unchanged, else restores or builds.
    fn open(&mut self, cancel: &CancelToken) -> Result<Volume, NtfsError> {
        let mut opened = Volume::open(self.drive.letter)?;
        // A worker exists only for a selected volume, so the create never reaches another one.
        self.journal
            .ensure(&mut opened, self.drive.letter, self.rebuilds.log());
        let serial = opened.serial()?;
        let same_volume = serial == self.drive.serial;
        self.drive.serial = serial;
        let held = {
            let mut state = self.shared.write().unwrap_or_else(PoisonError::into_inner);
            if !same_volume {
                state.index = None;
                state.search.forget_profile_ids();
            }
            state.index.is_some()
        };
        // A pending journal reset is checked first, so a reopen never skips it.
        if self.rebuilds.journal_reset_due(self.started.elapsed()) {
            return Ok(opened);
        }
        if held && !self.build_failed {
            return Ok(opened);
        }
        let mut built = false;
        let mut permit = None;
        let started = Instant::now();
        let index = restore_or_enumerate(file::load_with_ids(&self.path), self.drive, |reason| {
            built = true;
            let (index, held) = self.build(&mut opened, Cause::new(reason), cancel)?;
            permit = Some(held);
            Ok(index)
        })?;
        if built {
            self.publish_built(index);
            self.rebuilds.regular_build_published();
        } else {
            let entries = index.table.len();
            self.log_line(restored_line(self.drive.letter, entries, started.elapsed()));
            self.publish(index);
        }
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
        self.publish_built(index);
        self.rebuilds.regular_build_published();
        drop(permit);
        Ok(())
    }

    /// Tries a verification beside the live index and reports whether it was replaced.
    fn verify(
        &mut self,
        volume: &mut Volume,
        reason: &'static str,
        cancel: &CancelToken,
    ) -> Result<bool, NtfsError> {
        let attempted = self.build_candidate(volume, Cause::verification(), cancel);
        let Some((index, permit)) =
            self.rebuilds
                .verification_result(attempted, reason, Instant::now())?
        else {
            self.shared
                .write()
                .unwrap_or_else(PoisonError::into_inner)
                .status
                .progress = None;
            return Ok(false);
        };
        self.publish_built(index);
        drop(permit);
        Ok(true)
    }

    /// Publishes a completed index and starts its snapshot save clock.
    fn publish(&mut self, index: NtfsIndex) {
        replace(&self.shared, index);
        self.rebuilds.index_published();
        self.build_failed = false;
        self.clock.changed(self.started.elapsed());
    }

    /// Publishes a fresh build and saves it on the next tick: the old snapshot stays on disk
    /// until this save replaces it atomically, and a crash meanwhile replays from that snapshot.
    fn publish_built(&mut self, index: NtfsIndex) {
        self.publish(index);
        self.clock.built(self.started.elapsed());
        let mut state = self.shared.write().unwrap_or_else(PoisonError::into_inner);
        state.follow.last_full_build = Some(Instant::now());
        state.rebuild_due_at = None;
    }

    /// Attempts a regular full build and records its retry delay on error.
    fn build(
        &mut self,
        volume: &mut Volume,
        cause: Cause,
        cancel: &CancelToken,
    ) -> Result<(NtfsIndex, BuildPermit<'static>), NtfsError> {
        let attempted = self.build_candidate(volume, cause, cancel);
        // A reopen restores the snapshot on disk or builds; it never keeps this in-memory index.
        self.build_failed |= attempted
            .as_ref()
            .is_err_and(|error| !matches!(error, NtfsError::Cancelled));
        self.rebuilds.build_result(attempted)
    }

    /// Builds a candidate; the snapshot on disk is left alone, so a stop or a failed build keeps
    /// it for the next start and only the next save replaces it (IXH1).
    fn build_candidate(
        &mut self,
        volume: &mut Volume,
        cause: Cause,
        cancel: &CancelToken,
    ) -> Result<(NtfsIndex, BuildPermit<'static>), NtfsError> {
        self.rebuilds
            .started(&self.shared, cause, self.started.elapsed());
        if !cause.verification {
            self.clock.saved();
        }
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
        let hint = rebuild_hint(&self.shared);
        self.log_line(format_args!("build queued {}:", self.drive.letter));
        let permit = BUILDS.acquire(cancel)?;
        self.log_line(format_args!(
            "build start {}: hint_entries={}",
            self.drive.letter, hint.entries
        ));
        let continuity = self.rebuilds.continuity(cause);
        let enumerated = volume.enumerate_sized(hint, continuity, cancel)?;
        if cause.verification {
            self.clock.saved();
        }
        self.report_build(&enumerated, started);
        Ok((enumerated.index, permit))
    }

    /// Logs the finished build and schedules verification if the enumeration missed records.
    fn report_build(&mut self, enumerated: &Enumeration, started: Instant) {
        let finished = Instant::now();
        let duration = finished.duration_since(started);
        let entries = enumerated.index.table.len();
        self.log_line(format_args!(
            "build done {}: entries={entries} blocked_peak={} skipped={} ms={}",
            self.drive.letter,
            enumerated.blocked_peak,
            enumerated.skipped,
            duration.as_millis()
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
        self.rebuilds.finished(&self.shared, entries, duration);
        self.rebuilds
            .build_quality(enumerated.inexact_reason(), finished);
    }

    /// Saves a due snapshot, logging failures and returning them as I/O errors. While a rebuild
    /// waits for unresolved changes, the cursor stays before them so a restore replays them.
    fn save(&mut self, stopping: bool) -> io::Result<()> {
        let input = SaveInput {
            shared: &self.shared,
            drive: self.drive,
            path: &self.path,
            elapsed: self.started.elapsed(),
            stopping,
            built_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            cursor: self.rebuilds.save_cursor(),
        };
        let saved = save_snapshot(input, &mut self.clock);
        let letter = self.drive.letter;
        match &saved {
            Ok(Some(entries)) => {
                self.log_line(format_args!("snapshot saved {letter}: entries={entries}"));
            }
            Ok(None) => {}
            Err(error) => {
                let kind = file_error_kind(error);
                self.log_line(format_args!("snapshot save failed {letter}: {kind}"));
            }
        }
        saved.map(drop).map_err(io::Error::other)
    }

    fn log_line(&self, text: impl std::fmt::Display) {
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

/// Waits `delay` before the next tick: none for zero, one park up to 500 ms, else cancellable.
fn wait(cancel: &CancelToken, delay: Duration) {
    if delay.is_zero() {
        return;
    }
    if delay <= Duration::from_millis(500) {
        std::thread::park_timeout(delay);
    } else {
        super::bind_retry::wait_for_retry(cancel, delay);
    }
}

pub(super) fn record_error(shared: &Shared, message: String) {
    let mut state = shared.write().unwrap_or_else(PoisonError::into_inner);
    if !state.status.errors.contains(&message) {
        state.status.errors.truncate(3);
        state.status.errors.push(message);
    }
}
