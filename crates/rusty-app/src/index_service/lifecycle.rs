use std::time::Duration;

use super::config::Drive;

use rusty_index::file::{self, FileError, Header};
use rusty_index::ntfs::{NtfsError, NtfsIndex};
use rusty_index::pipe::CancelToken;
use rusty_index::table::NameTable;
use rusty_index::usn::FrnMap;

pub(super) const UPDATE_FAILURE: &str = "updating volume index: ";
pub(super) const UNRESOLVED: &str = "enumeration has unresolved ";
pub(super) const SAVE_FAILURE: &str = "saving volume index: ";

pub(super) fn clear_failures(errors: &mut Vec<String>, prefix: &str) {
    errors.retain(|message| !message.starts_with(prefix));
}

pub(super) fn report_unknown_sizes(errors: &mut Vec<String>, count: u64) {
    const PREFIX: &str = "sizes unknown: ";
    clear_failures(errors, PREFIX);
    if count != 0 {
        errors.push(format!("{PREFIX}{count}"));
    }
}

pub(super) fn guard_worker(
    cancel: &CancelToken,
    report: impl FnOnce(String),
    run: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<()> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
        Ok(result) => result,
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("non-string panic");
            let message = format!("index worker panic: {message}");
            cancel.cancel();
            report(message.clone());
            Err(std::io::Error::other(message))
        }
    }
}

pub(super) fn restore_or_enumerate(
    saved: Result<(Header, NameTable, FrnMap), FileError>,
    drive: Drive,
    enumerate: impl FnOnce() -> Result<NtfsIndex, NtfsError>,
) -> Result<NtfsIndex, NtfsError> {
    let restored = saved
        .ok()
        .filter(|(header, _, _)| header.root == format!("{}:", drive.letter))
        .and_then(|saved| {
            let journal_id = saved.0.journal_id;
            NtfsIndex::restore(saved, drive.serial, journal_id).ok()
        });
    if let Some(index) = restored {
        return Ok(index);
    }
    enumerate()
}

#[derive(Default)]
pub(super) struct SaveClock {
    changed: Option<Duration>,
    retry_at: Option<Duration>,
    failed: bool,
    large_batch: bool,
}

impl SaveClock {
    pub fn changed(&mut self, now: Duration) {
        self.changed = Some(now);
    }

    pub fn batch(&mut self, now: Duration, records: usize) {
        self.changed(now);
        self.large_batch |= records > 10_000;
    }

    pub fn due<T>(&self, now: Duration, snapshot: Option<&T>, stopping: bool) -> bool {
        snapshot.is_some()
            && self.changed.is_some_and(|changed| {
                stopping
                    || self.retry_at.map_or_else(
                        || {
                            self.large_batch
                                || now.saturating_sub(changed) >= Duration::from_secs(30)
                        },
                        |retry| now >= retry,
                    )
            })
    }

    pub fn saved(&mut self) {
        self.changed = None;
        self.retry_at = None;
        self.failed = false;
        self.large_batch = false;
    }

    pub fn failed(&mut self, now: Duration) {
        self.retry_at = Some(now + Duration::from_secs(if self.failed { 300 } else { 30 }));
        self.failed = true;
    }

    pub fn save(
        &mut self,
        path: &std::path::Path,
        snapshot: (Header, &NameTable, &FrnMap),
        now: Duration,
    ) -> Result<(), FileError> {
        let (header, table, ids) = snapshot;
        if let Err(error) = file::save_with_ids(path, &header, table, ids) {
            self.failed(now);
            return Err(error);
        }
        self.saved();
        Ok(())
    }
}
