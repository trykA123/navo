use std::fmt;
use std::sync::PoisonError;
use std::time::Duration;

use navo_index::file::FileError;
use navo_index::ntfs::{NtfsError, UnresolvedChange};
use navo_index::pipe::{BuildReason, VolumeBuilds};
use navo_index::table::TableChange;

use super::answer::Shared;
use super::service_log::Log;

/// At most one full build per volume starts within this window; later causes wait for its end.
pub(super) const REBUILD_WINDOW: Duration = Duration::from_secs(600);

/// Unresolved changes of one journal batch, by kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Unresolved {
    blocked_add: u64,
    blocked_rename: u64,
    attributes: u64,
    size: u64,
    other: u64,
}

impl Unresolved {
    pub fn count(changes: &[UnresolvedChange]) -> Self {
        let mut counts = Self::default();
        for unresolved in changes {
            let count = match unresolved.change {
                TableChange::Add { .. } => &mut counts.blocked_add,
                TableChange::Rename { .. } => &mut counts.blocked_rename,
                TableChange::Attributes { .. } => &mut counts.attributes,
                TableChange::Size { .. } => &mut counts.size,
                TableChange::Remove { .. } | TableChange::JournalReset => &mut counts.other,
            };
            *count += 1;
        }
        counts
    }

    /// Attribute and size changes for files outside the index change nothing in it.
    pub fn needs_rebuild(&self) -> bool {
        self.blocked_add + self.blocked_rename + self.other > 0
    }

    fn total(&self) -> u64 {
        self.blocked_add + self.blocked_rename + self.attributes + self.size + self.other
    }
}

impl fmt::Display for Unresolved {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "blocked_add={} blocked_rename={} attributes={} size={} other={}",
            self.blocked_add, self.blocked_rename, self.attributes, self.size, self.other
        )
    }
}

/// Why a full build starts; `unresolved` is set when journal changes caused it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Cause {
    pub reason: BuildReason,
    pub unresolved: Option<Unresolved>,
}

impl Cause {
    pub fn new(reason: BuildReason) -> Self {
        Self {
            reason,
            unresolved: None,
        }
    }
}

impl fmt::Display for Cause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "reason={}", self.reason.label())?;
        match self.unresolved {
            Some(unresolved) => write!(formatter, " {unresolved}"),
            None => Ok(()),
        }
    }
}

/// Decides when a volume's full builds start and keeps their counters and log lines.
pub(super) struct Rebuilds {
    letter: char,
    log: Log,
    last_start: Option<Duration>,
    deferred: Option<Cause>,
}

impl Rebuilds {
    pub fn new(letter: char, log: Log) -> Self {
        Self {
            letter,
            log,
            last_start: None,
            deferred: None,
        }
    }

    /// Turns one follow result into the full build that must start now, if any.
    pub fn after_follow(
        &mut self,
        shared: &Shared,
        followed: Result<Vec<UnresolvedChange>, NtfsError>,
        now: Duration,
    ) -> Result<Option<Cause>, NtfsError> {
        let changes = match followed {
            Ok(changes) if changes.is_empty() => return Ok(None),
            Ok(changes) => changes,
            Err(NtfsError::Reenumerate) => {
                return Ok(self.request(shared, Cause::new(BuildReason::JournalReset), now));
            }
            Err(error) => return Err(error),
        };
        let unresolved = Unresolved::count(&changes);
        let needs_rebuild = unresolved.needs_rebuild();
        update(shared, |builds| {
            builds.unresolved_batches += 1;
            if !needs_rebuild {
                builds.dropped_changes += unresolved.total();
            }
        });
        if !needs_rebuild {
            tracing::debug!(volume = %self.letter, %unresolved, "dropped changes for files outside the index");
            return Ok(None);
        }
        let cause = Cause {
            reason: BuildReason::UnresolvedChanges,
            unresolved: Some(unresolved),
        };
        Ok(self.request(shared, cause, now))
    }

    fn request(&mut self, shared: &Shared, cause: Cause, now: Duration) -> Option<Cause> {
        let Some(since) = self.last_start.map(|start| now.saturating_sub(start)) else {
            return Some(cause);
        };
        if since >= REBUILD_WINDOW {
            return Some(cause);
        }
        if self.deferred.is_none() {
            self.deferred = Some(cause);
            update(shared, |builds| builds.deferred_rebuilds += 1);
            let wait = REBUILD_WINDOW.saturating_sub(since).as_secs();
            self.log.line(format_args!(
                "rebuild deferred {}: {cause} wait_s={wait}",
                self.letter
            ));
        }
        None
    }

    /// The deferred build, once the window since the last build start has passed.
    pub fn due(&self, now: Duration) -> Option<Cause> {
        self.deferred.filter(|_| {
            self.last_start
                .is_none_or(|start| now.saturating_sub(start) >= REBUILD_WINDOW)
        })
    }

    /// True while the index misses changes a deferred build will bring back.
    pub fn is_deferred(&self) -> bool {
        self.deferred.is_some()
    }

    pub fn started(&mut self, shared: &Shared, cause: Cause, now: Duration) {
        self.last_start = Some(now);
        self.deferred = None;
        update(shared, |builds| {
            builds.full_builds += 1;
            builds.last_reason = Some(cause.reason);
        });
        self.log
            .line(format_args!("full build start {}: {cause}", self.letter));
    }

    pub fn finished(&self, shared: &Shared, entries: usize, duration: Duration) {
        let milliseconds = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
        update(shared, |builds| builds.last_duration_ms = milliseconds);
        self.log.line(format_args!(
            "full build done {}: entries={entries} duration_ms={milliseconds}",
            self.letter
        ));
    }

    #[cfg(windows)]
    pub fn log(&self) -> &Log {
        &self.log
    }
}

fn update(shared: &Shared, change: impl FnOnce(&mut VolumeBuilds)) {
    change(
        &mut shared
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .builds,
    );
}

/// A log-safe error kind: static text and OS codes, never names.
pub(super) fn error_kind(error: &NtfsError) -> String {
    match error {
        NtfsError::Io { operation, source } => format!("io op=\"{operation}\" {}", io_kind(source)),
        NtfsError::InvalidVolume => "invalid_volume".into(),
        NtfsError::InvalidResponse(what) => format!("invalid_response \"{what}\""),
        NtfsError::Snapshot(error) => file_error_kind(error),
        NtfsError::Parse(_) => "parse".into(),
        NtfsError::Table(_) => "table".into(),
        NtfsError::Reenumerate => "reenumerate".into(),
        NtfsError::FileGone => "file_gone".into(),
        NtfsError::Cancelled => "cancelled".into(),
    }
}

pub(super) fn file_error_kind(error: &FileError) -> String {
    match error {
        FileError::Io(source) => format!("snapshot_io {}", io_kind(source)),
        FileError::Allocation(_) => "snapshot_allocation".into(),
        FileError::BadMagic => "snapshot_bad_magic".into(),
        FileError::UnsupportedVersion { found, .. } => format!("snapshot_version found={found}"),
        FileError::BadLength => "snapshot_bad_length".into(),
        FileError::BadChecksum => "snapshot_bad_checksum".into(),
        FileError::InvalidData(what) => format!("snapshot_invalid \"{what}\""),
    }
}

pub(super) fn io_kind(error: &std::io::Error) -> String {
    match error.raw_os_error() {
        Some(code) => format!("os_error={code}"),
        None => format!("kind={:?}", error.kind()),
    }
}

#[cfg(test)]
#[path = "rebuild_tests.rs"]
mod tests;
