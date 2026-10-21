use std::time::Duration;

use super::config::Drive;

use rusty_index::file::{FileError, Header};
use rusty_index::ntfs::{NtfsError, NtfsIndex};
use rusty_index::pipe::CancelToken;
use rusty_index::table::NameTable;
use rusty_index::usn::FrnMap;

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
    resume: impl FnOnce(&mut NtfsIndex) -> Result<(), NtfsError>,
    enumerate: impl FnOnce() -> Result<NtfsIndex, NtfsError>,
) -> Result<NtfsIndex, NtfsError> {
    let restored = saved
        .ok()
        .filter(|(header, _, _)| header.root == format!("{}:", drive.letter))
        .and_then(|saved| {
            let journal_id = saved.0.journal_id;
            NtfsIndex::restore(saved, drive.serial, journal_id).ok()
        });
    if let Some(mut index) = restored
        && resume(&mut index).is_ok()
    {
        return Ok(index);
    }
    enumerate()
}

#[derive(Default)]
pub(super) struct SaveClock {
    changed: Option<Duration>,
    retry_at: Option<Duration>,
    failed: bool,
}

impl SaveClock {
    pub fn changed(&mut self, now: Duration) {
        self.changed = Some(now);
    }

    pub fn due<T>(&self, now: Duration, snapshot: Option<T>, stopping: bool) -> bool {
        snapshot.is_some()
            && self.changed.is_some_and(|changed| {
                stopping
                    || self.retry_at.map_or_else(
                        || now.saturating_sub(changed) >= Duration::from_secs(30),
                        |retry| now >= retry,
                    )
            })
    }

    pub fn saved(&mut self) {
        self.changed = None;
        self.retry_at = None;
        self.failed = false;
    }

    pub fn failed(&mut self, now: Duration) {
        self.retry_at = Some(now + Duration::from_secs(if self.failed { 300 } else { 30 }));
        self.failed = true;
    }
}
