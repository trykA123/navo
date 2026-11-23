//! Tracks full build timing, retry delays and journal rebuild reasons for one volume.

use std::fmt;
use std::sync::PoisonError;
use std::time::{Duration, Instant};

use navo_index::file::FileError;
use navo_index::ntfs::{InexactBuild, NtfsError, UnresolvedChange};
use navo_index::pipe::{BuildReason, VolumeBuilds};

use super::answer::Shared;
use super::service_log::Log;
use super::verify_rebuild::VerifyRebuild;

#[path = "unresolved_refs.rs"]
mod unresolved_refs;
pub(super) use unresolved_refs::Unresolved;

/// At most one full build per volume starts within this window; later causes wait for its end.
pub(super) const REBUILD_WINDOW: Duration = Duration::from_secs(600);

/// Counts consecutive full builds that failed.
#[derive(Default)]
struct BuildBackoff(u32);

impl BuildBackoff {
    /// Records a build failure and returns its retry delay, capped at 300 seconds.
    fn failed(&mut self) -> Duration {
        self.0 = self.0.saturating_add(1);
        Duration::from_secs((1u64 << self.0.min(9)).min(300))
    }

    /// Clears the failure streak after a successful build.
    fn succeeded(&mut self) {
        self.0 = 0;
    }
}

/// Why a full build starts; `unresolved` is set when journal changes caused it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Cause {
    pub reason: BuildReason,
    pub unresolved: Option<Unresolved>,
    pub verification: bool,
}

impl Cause {
    pub fn new(reason: BuildReason) -> Self {
        Self {
            reason,
            unresolved: None,
            verification: false,
        }
    }

    /// Identifies a scheduled verification so the worker preserves the live generation on failure.
    pub fn verification() -> Self {
        Self {
            verification: true,
            ..Self::new(BuildReason::UnresolvedChanges)
        }
    }
}

impl fmt::Display for Cause {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = if self.verification {
            "verification"
        } else {
            self.reason.label()
        };
        write!(formatter, "reason={reason}")?;
        match self.unresolved {
            Some(unresolved) => write!(formatter, " {unresolved}"),
            None => Ok(()),
        }
    }
}

/// A queued full build and its deadline in seconds since the worker started.
#[derive(Clone, Copy)]
struct Deferred {
    cause: Cause,
    deadline: Duration,
}

/// Decides when a volume's full builds start and keeps their counters and log lines.
pub(super) struct Rebuilds {
    letter: char,
    log: Log,
    last_start: Option<Duration>,
    deferred: Option<Deferred>,
    backoff: BuildBackoff,
    retry_delay: Option<Duration>,
    verification: VerifyRebuild,
    verification_reason: Option<&'static str>,
}

impl Rebuilds {
    pub fn new(letter: char, log: Log) -> Self {
        Self {
            letter,
            log,
            last_start: None,
            deferred: None,
            backoff: BuildBackoff::default(),
            retry_delay: None,
            verification: VerifyRebuild::default(),
            verification_reason: None,
        }
    }

    /// Recomputes verification from the finished build: exact clears it, inexact waits from `now`.
    pub fn build_quality(&mut self, reason: Option<InexactBuild>, now: Instant) {
        if reason.is_some() && self.verification.is_pending() {
            // An older deadline checked the replaced index; restart the delay from this build.
            self.verification.exact();
        }
        match reason {
            Some(InexactBuild::Cycle) => self.schedule_verification(now, "build_cycle"),
            Some(InexactBuild::HeldCap) => self.schedule_verification(now, "held_cap"),
            None => {
                self.verification.exact();
                self.verification_reason = None;
            }
        }
    }

    /// Schedules and logs one verification unless the drive already has one waiting.
    pub fn schedule_verification(&mut self, now: Instant, reason: &'static str) {
        if self.verification.is_pending() {
            return;
        }
        let deadline = self.verification.inexact(now);
        self.verification_reason = Some(reason);
        let minutes = deadline.duration_since(now).as_secs() / 60;
        self.log.line(format_args!(
            "verify scheduled {}: in {minutes} min ({reason})",
            self.letter
        ));
    }

    /// Takes the reason for a ready verification when no journal reset is pending.
    pub fn verification_due(&mut self, now: Instant) -> Option<&'static str> {
        // A waiting reset rebuilds anyway; verifying first would start a second full build.
        if !self.journal_lost() && self.verification.take_due(now) {
            return self.verification_reason.take();
        }
        None
    }

    /// Finishes a verification attempt and returns any replacement index.
    pub fn verification_result<T>(
        &mut self,
        attempted: Result<T, NtfsError>,
        reason: &'static str,
        now: Instant,
    ) -> Result<Option<T>, NtfsError> {
        match attempted {
            Ok(candidate) => Ok(Some(candidate)),
            Err(NtfsError::Cancelled) => Err(NtfsError::Cancelled),
            Err(error) => {
                self.backoff.failed();
                self.log.line(format_args!(
                    "verify failed {}: {}",
                    self.letter,
                    error_kind(&error)
                ));
                self.schedule_verification(now, reason);
                Ok(None)
            }
        }
    }

    /// Returns a regular full-build result and records the retry delay on failure.
    pub fn build_result<T>(&mut self, result: Result<T, NtfsError>) -> Result<T, NtfsError> {
        if result
            .as_ref()
            .is_err_and(|error| !matches!(error, NtfsError::Cancelled))
        {
            self.retry_delay = Some(self.backoff.failed());
        }
        result
    }

    /// Takes the delay for the next full-build retry.
    pub fn take_retry_delay(&mut self) -> Option<Duration> {
        let delay = self.retry_delay.take()?;
        self.log.line(format_args!(
            "build retry {}: in {} s after {} failures",
            self.letter,
            delay.as_secs(),
            self.backoff.0
        ));
        Some(delay)
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
            Err(error @ (NtfsError::Reenumerate(_) | NtfsError::LinkRefresh(_))) => {
                if self.journal_lost() {
                    return Ok(None);
                }
                self.log.line(format_args!(
                    "update error {}: {}",
                    self.letter,
                    error_kind(&error)
                ));
                return Ok(self.request_paused(shared, Cause::new(BuildReason::JournalReset), now));
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
            verification: false,
        };
        Ok(self.request(shared, cause, now))
    }

    /// Pauses replay and requests a build after a batch failed to apply to the live index.
    /// The index is never saved again: the pause holds until a regular build is published.
    pub fn apply_failed(
        &mut self,
        shared: &Shared,
        error: &NtfsError,
        now: Duration,
    ) -> Option<Cause> {
        if self.journal_lost() {
            return None;
        }
        let kind = error_kind(error);
        self.log
            .line(format_args!("update error {}: apply {kind}", self.letter));
        let cause = Cause::new(BuildReason::ApplyFailed);
        let ready = self.request_paused(shared, cause, now);
        // Fail closed: saves stay blocked even before the worker starts the build.
        if ready.is_some() {
            self.deferred = Some(Deferred {
                cause,
                deadline: now,
            });
        }
        ready
    }

    /// Gives a replay-pausing cause priority without counting an existing deferral twice.
    fn request_paused(&mut self, shared: &Shared, cause: Cause, now: Duration) -> Option<Cause> {
        if let Some(deferred) = &mut self.deferred {
            deferred.cause = cause;
        }
        self.request(shared, cause, now)
    }

    fn request(&mut self, shared: &Shared, cause: Cause, now: Duration) -> Option<Cause> {
        let Some(start) = self.last_start else {
            return Some(cause);
        };
        let deadline = start.saturating_add(REBUILD_WINDOW);
        if now >= deadline {
            return Some(cause);
        }
        if self.deferred.is_none() {
            self.deferred = Some(Deferred { cause, deadline });
            update(shared, |builds| builds.deferred_rebuilds += 1);
            let wait = (deadline - now).as_secs();
            self.log.line(format_args!(
                "rebuild deferred {}: {cause} wait_s={wait}",
                self.letter
            ));
        }
        None
    }

    /// Returns the deferred build once its deadline has passed.
    pub fn due(&self, now: Duration) -> Option<Cause> {
        self.deferred
            .filter(|deferred| now >= deferred.deadline)
            .map(|deferred| deferred.cause)
    }

    /// True while a deferred rebuild or verification may restore missing entries.
    pub fn is_deferred(&self) -> bool {
        self.deferred.is_some() || self.verification.is_pending()
    }

    /// Returns whether journal replay must wait for a deferred rebuild (journal lost or apply failed).
    pub fn journal_lost(&self) -> bool {
        self.deferred
            .is_some_and(|deferred| pauses_replay(deferred.cause.reason))
    }

    /// Returns whether the replay-pausing rebuild can run at this worker elapsed time.
    pub fn journal_reset_due(&self, now: Duration) -> bool {
        self.journal_lost() && self.due(now).is_some()
    }

    pub fn started(&mut self, shared: &Shared, cause: Cause, now: Duration) {
        self.last_start = Some(now);
        if pauses_replay(cause.reason) {
            // The worker waits on build backoff before retrying this ready reset.
            self.deferred = Some(Deferred {
                cause,
                deadline: now,
            });
        } else if !self.journal_lost() {
            self.deferred = None;
        }
        update(shared, |builds| {
            builds.full_builds += 1;
            builds.last_reason = Some(cause.reason);
        });
        self.log
            .line(format_args!("full build start {}: {cause}", self.letter));
    }

    pub fn finished(&mut self, shared: &Shared, entries: usize, duration: Duration) {
        self.backoff.succeeded();
        self.retry_delay = None;
        let milliseconds = u64::try_from(duration.as_millis()).unwrap_or(u64::MAX);
        update(shared, |builds| builds.last_duration_ms = milliseconds);
        self.log.line(format_args!(
            "full build done {}: entries={entries} duration_ms={milliseconds}",
            self.letter
        ));
    }

    /// Clears the pending journal reset after a regular full build replaces the live index.
    pub fn regular_build_published(&mut self) {
        if self.journal_lost() {
            self.deferred = None;
        }
    }

    #[cfg(windows)]
    pub fn log(&self) -> &Log {
        &self.log
    }
}

/// True for causes whose old index must neither replay the journal nor be saved until rebuilt.
fn pauses_replay(reason: BuildReason) -> bool {
    matches!(reason, BuildReason::JournalReset | BuildReason::ApplyFailed)
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
        NtfsError::Reenumerate(cause) => format!("reenumerate({cause})"),
        NtfsError::LinkRefresh(failed) => format!("reenumerate(hard link) {}", error_kind(failed)),
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
