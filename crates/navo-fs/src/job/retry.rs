//! Retries a copy whose source another program holds open (a sharing violation, Windows error 32).
//! Each retry restarts the step through the normal temporary-then-rename path, never a partial file.

use std::time::Duration;

use navo_core::{FsError, FsErrorKind};

use super::aside::presence;
use super::copy::{Copied, Copier, CopyHooks, CopyStep, failure};

/// The waits before the first, second and third retry; after the last, the error stands.
pub(super) const WAITS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];

/// Copies `step` like `copy::copy`, retrying a source in use after each of `WAITS`.
/// Fails with the last attempt's error, or `Cancelled` when cancel wakes a wait.
pub(super) fn copy(
    copier: &Copier<'_>,
    step: &CopyStep<'_>,
    hooks: CopyHooks<'_>,
) -> Result<Copied, FsError> {
    let CopyHooks { progress, created } = hooks;
    for delay in WAITS {
        let mut wrote = false;
        let mut counting = |bytes: u64| {
            wrote |= bytes > 0;
            progress(bytes);
        };
        let attempt = CopyHooks {
            progress: &mut counting,
            created: &mut *created,
        };
        match super::copy::copy(copier, step, attempt) {
            Err(error) if !wrote && may_retry(copier, step, &error) => {
                if copier.ops.wait_to_retry(delay, copier.cancel) {
                    return Err(failure(FsErrorKind::Cancelled, step.target));
                }
            }
            other => return other,
        }
    }
    super::copy::copy(copier, step, CopyHooks { progress, created })
}

/// True when `error` is a sharing violation on a same-provider source and the step's temporary
/// file is absent, so a new attempt starts from nothing. A stream from another provider may not
/// be readable twice, so it is never retried. A failed presence check means no retry.
fn may_retry(copier: &Copier<'_>, step: &CopyStep<'_>, error: &FsError) -> bool {
    !super::stream::crosses(step)
        && error.kind == FsErrorKind::SharingViolation
        && error
            .location
            .as_ref()
            .is_some_and(|at| at.same_place(step.source))
        && !copier.cancel.is_cancelled()
        && copier
            .temporary(step)
            .and_then(|temp| presence(copier.ops, &temp))
            .is_ok_and(|found| found.is_none())
}
