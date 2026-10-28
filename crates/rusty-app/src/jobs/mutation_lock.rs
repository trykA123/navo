use std::fs::File;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use rusty_core::FsErrorKind;
use rusty_fs::CancelToken;

mod path;

#[cfg(test)]
pub(crate) fn test_file(data: &Path) -> File {
    path::open(data).unwrap()
}

pub(crate) const WAIT: Duration = Duration::from_secs(30);
const POLL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LeaseError {
    Busy,
    Cancelled,
    Failed(String),
}

impl LeaseError {
    pub(crate) fn kind(&self) -> FsErrorKind {
        match self {
            Self::Busy => FsErrorKind::Timeout,
            Self::Cancelled => FsErrorKind::Cancelled,
            Self::Failed(_) => FsErrorKind::AccessDenied,
        }
    }
}

impl From<LeaseError> for String {
    fn from(error: LeaseError) -> Self {
        match error {
            LeaseError::Busy => rusty_l10n::tr("job-profile-busy"),
            LeaseError::Cancelled => rusty_l10n::tr("job-wait-cancelled"),
            LeaseError::Failed(message) => message,
        }
    }
}

pub(crate) struct Wait<'a> {
    pub cancel: Option<&'a CancelToken>,
    pub waiting: Option<&'a mut dyn FnMut(bool)>,
    pub limit: Duration,
}

impl Wait<'_> {
    pub(crate) fn bounded() -> Self {
        Self {
            cancel: None,
            waiting: None,
            limit: WAIT,
        }
    }

    fn announce(&mut self, waiting: bool) {
        if let Some(report) = self.waiting.as_mut() {
            report(waiting);
        }
    }

    fn pause(&self, remaining: Duration) {
        let pause = remaining.min(POLL);
        match self.cancel {
            Some(cancel) => {
                cancel.wait_cancelled(pause);
            }
            None => std::thread::park_timeout(pause),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Mode {
    Shared,
    Exclusive,
}

pub(crate) fn lease(data: &Path, mode: Mode, wait: Wait<'_>) -> Result<File, LeaseError> {
    acquire(path::open(data).map_err(LeaseError::Failed)?, mode, wait)
}

pub(crate) fn claim_recovery(data: &Path, experimental: bool) -> Result<Option<File>, String> {
    experimental
        .then(|| lease(data, Mode::Exclusive, Wait::bounded()))
        .transpose()
        .map_err(String::from)
}

impl super::JournalLog {
    pub(super) fn claim_lease(
        &mut self,
        operation: &mut rusty_ui::browser::LoggedOperation<'_>,
    ) -> Result<(), String> {
        if !self.experimental_retention || self.global_lease.is_some() {
            return Ok(());
        }
        let Some(dirs) = &self.profile else {
            return Err(rusty_l10n::tr("job-profile-unavailable"));
        };
        let mode = if self.retained.is_some()
            || self.overwrites.is_some()
            || operation.plan.steps.iter().any(|step| {
                matches!(
                    step,
                    rusty_core::Step::Rename { .. }
                        | rusty_core::Step::Recycle { .. }
                        | rusty_core::Step::RemoveFolder { .. }
                )
            }) {
            Mode::Exclusive
        } else {
            Mode::Shared
        };
        let wait = Wait {
            cancel: Some(operation.cancel),
            waiting: Some(&mut *operation.waiting),
            limit: WAIT,
        };
        self.global_lease = Some(lease(&dirs.data, mode, wait)?);
        Ok(())
    }
}

fn acquire(file: File, mode: Mode, mut wait: Wait<'_>) -> Result<File, LeaseError> {
    let deadline = Instant::now() + wait.limit;
    let mut is_announced = false;
    let result = loop {
        if wait.cancel.is_some_and(CancelToken::is_cancelled) {
            break Err(LeaseError::Cancelled);
        }
        match try_lock(&file, mode) {
            Ok(true) => break Ok(()),
            Ok(false) => {}
            Err(error) => break Err(LeaseError::Failed(error.to_string())),
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break Err(LeaseError::Busy);
        }
        if !is_announced {
            wait.announce(true);
            is_announced = true;
        }
        wait.pause(remaining);
    };
    if is_announced {
        wait.announce(false);
    }
    result.map(|()| file)
}

fn try_lock(file: &File, mode: Mode) -> io::Result<bool> {
    let result = match mode {
        Mode::Shared => file.try_lock_shared(),
        Mode::Exclusive => file.try_lock(),
    };
    match result {
        Ok(()) => Ok(true),
        Err(error) => {
            let error = io::Error::from(error);
            if error.kind() == io::ErrorKind::WouldBlock {
                Ok(false)
            } else {
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests;
