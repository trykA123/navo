#[cfg(any(windows, test))]
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(any(windows, test))]
use std::sync::mpsc;
#[cfg(any(windows, test))]
use std::time::Duration;

#[cfg(windows)]
use crate::index_service;
#[cfg(windows)]
use crate::logging;
#[cfg(any(windows, test))]
use crate::logging::UNINSTALL_TARGET;
#[cfg(any(windows, test))]
use navo_index_service::Elevated;
#[cfg(windows)]
use navo_store::StoreDirs;

/// The uninstall callback's wait for the elevated remove, which has `REMOVE_DEADLINE`.
#[cfg(any(windows, test))]
const REMOVE_BUDGET: Duration = Duration::from_secs(25);

/// The uninstall callback's wait for the PDF helper profile removal, well inside Velopack's
/// hook timeout so the index service removal and log shutdown still run.
#[cfg(any(windows, test))]
const PROFILE_BUDGET: Duration = Duration::from_secs(5);

static RESTARTED: AtomicBool = AtomicBool::new(false);

/// Runs Velopack's start-up hooks. Staged packages apply only through `updater::apply`.
pub(crate) fn run() {
    let mut app = velopack::VelopackApp::build()
        .set_auto_apply_on_startup(false)
        .on_restarted(|_| RESTARTED.store(true, Ordering::Relaxed));
    #[cfg(windows)]
    {
        app = app.on_before_uninstall_fast_callback(|_| before_uninstall());
    }
    app.run();
}

/// True when Velopack started this process right after applying a package.
pub(crate) fn restarted() -> bool {
    RESTARTED.load(Ordering::Relaxed)
}

#[cfg(any(windows, test))]
#[derive(Debug, PartialEq, Eq)]
enum Removal {
    NotInstalled,
    Removed,
    /// Removed; Windows deletes the last files at the next restart.
    Deferred,
    Skipped(String),
}

#[cfg(any(windows, test))]
fn remove_when_installed(
    installed: impl FnOnce() -> io::Result<bool>,
    remove: impl FnOnce() -> Result<Elevated, String> + Send + 'static,
    wait: Duration,
) -> Removal {
    match installed() {
        Ok(false) => return Removal::NotInstalled,
        Ok(true) => {}
        Err(error) => return Removal::Skipped(format!("cannot query the service: {error}")),
    }
    match run_bounded("navo-uninstall-service", remove, wait) {
        Ok(Ok(Elevated::Done)) => Removal::Removed,
        Ok(Ok(Elevated::Deferred)) => Removal::Deferred,
        Ok(Err(error)) | Err(error) => Removal::Skipped(error),
    }
}

/// Runs `work` on a named worker and waits at most `wait` for its answer.
///
/// A refused thread or a missed deadline comes back as an error text; a late worker is left
/// to finish on its own so the uninstall hook keeps its time budget.
#[cfg(any(windows, test))]
fn run_bounded<T: Send + 'static>(
    name: &str,
    work: impl FnOnce() -> T + Send + 'static,
    wait: Duration,
) -> Result<T, String> {
    let (done, finished) = mpsc::channel();
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let _ = done.send(work());
        })
        .map_err(|error| format!("cannot start the worker: {error}"))?;
    finished
        .recv_timeout(wait)
        .map_err(|_| format!("no answer within {} s", wait.as_secs()))
}

/// Removes the PDF helper profile, then the index service, and closes diagnostics last.
#[cfg(windows)]
fn before_uninstall() {
    let dirs = StoreDirs::detect();
    let _diagnostics = logging::install(dirs.as_ref().map(|dirs| dirs.data.as_path()));
    before_uninstall_with(
        navo_shell::remove_pdf_container_profile,
        remove_index_service,
        PROFILE_BUDGET,
    );
    logging::shutdown();
}

/// Removes the profile and then the service, so a failed or stalled profile removal never
/// blocks the uninstall: the profile attempt gets at most `profile_wait`.
#[cfg(any(windows, test))]
fn before_uninstall_with(
    remove_profile: impl FnOnce() -> Result<(), i32> + Send + 'static,
    remove_service: impl FnOnce(),
    profile_wait: Duration,
) {
    match run_bounded("navo-uninstall-profile", remove_profile, profile_wait) {
        Ok(Ok(())) => tracing::info!(
            target: UNINSTALL_TARGET,
            "uninstall: PDF helper profile removed"
        ),
        Ok(Err(hresult)) => tracing::warn!(
            target: UNINSTALL_TARGET,
            hresult,
            "uninstall: PDF helper profile left in place"
        ),
        Err(reason) => tracing::warn!(
            target: UNINSTALL_TARGET,
            reason,
            "uninstall: PDF helper profile left in place"
        ),
    }
    remove_service();
}

#[cfg(windows)]
fn remove_index_service() {
    let outcome = remove_when_installed(
        || {
            let query = index_service::query()?;
            Ok(query.running.is_some() || query.legacy_present)
        },
        || {
            index_service::request_elevation(index_service::Action::Remove)
                .map_err(|error| error.to_string())
        },
        REMOVE_BUDGET,
    );
    match outcome {
        Removal::NotInstalled => {
            tracing::info!(target: UNINSTALL_TARGET, "uninstall: no index service to remove");
        }
        Removal::Removed => {
            tracing::info!(target: UNINSTALL_TARGET, "uninstall: index service removed");
        }
        Removal::Deferred => {
            tracing::info!(
                target: UNINSTALL_TARGET,
                "uninstall: index service removed; its last files go at the next restart"
            );
        }
        Removal::Skipped(reason) => {
            tracing::warn!(
                target: UNINSTALL_TARGET,
                reason,
                "uninstall: index service left in place"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    #[test]
    fn nothing_is_removed_or_elevated_when_no_service_exists() {
        let called = Arc::new(AtomicBool::new(false));
        let seen = called.clone();
        let outcome = remove_when_installed(
            || Ok(false),
            move || {
                seen.store(true, Ordering::SeqCst);
                Ok(Elevated::Done)
            },
            Duration::from_secs(1),
        );
        assert_eq!(outcome, Removal::NotInstalled);
        assert!(!called.load(Ordering::SeqCst));
    }

    #[test]
    fn a_refused_elevation_is_reported_as_skipped() {
        let outcome = remove_when_installed(
            || Ok(true),
            || Err("cancelled".into()),
            Duration::from_secs(1),
        );
        assert_eq!(outcome, Removal::Skipped("cancelled".into()));
    }

    #[test]
    fn a_removal_that_outlives_the_budget_does_not_hold_the_uninstall() {
        let outcome = remove_when_installed(
            || Ok(true),
            || {
                std::thread::sleep(Duration::from_secs(3));
                Ok(Elevated::Done)
            },
            Duration::from_millis(100),
        );
        assert!(matches!(outcome, Removal::Skipped(_)));
    }

    #[test]
    fn a_failed_query_skips_the_removal() {
        let outcome = remove_when_installed(
            || Err(io::ErrorKind::PermissionDenied.into()),
            || Ok(Elevated::Done),
            Duration::from_secs(1),
        );
        assert!(matches!(outcome, Removal::Skipped(_)));
    }

    #[test]
    fn a_deferred_removal_is_removed_not_skipped() {
        let outcome = remove_when_installed(
            || Ok(true),
            || Ok(Elevated::Deferred),
            Duration::from_secs(1),
        );
        assert_eq!(outcome, Removal::Deferred);
        let done =
            remove_when_installed(|| Ok(true), || Ok(Elevated::Done), Duration::from_secs(1));
        assert_eq!(done, Removal::Removed);
    }

    #[test]
    fn a_slow_removal_is_skipped_with_no_answer() {
        let outcome = remove_when_installed(
            || Ok(true),
            || {
                std::thread::sleep(Duration::from_secs(3));
                Ok(Elevated::Deferred)
            },
            Duration::from_secs(1),
        );
        assert_eq!(outcome, Removal::Skipped("no answer within 1 s".into()));
    }

    #[test]
    fn the_profile_is_removed_before_the_index_service() {
        let steps = Arc::new(Mutex::new(Vec::new()));
        let profile = steps.clone();
        before_uninstall_with(
            move || {
                profile.lock().unwrap().push("profile");
                Ok(())
            },
            || steps.lock().unwrap().push("service"),
            Duration::from_secs(1),
        );
        assert_eq!(*steps.lock().unwrap(), ["profile", "service"]);
    }

    #[test]
    fn a_failed_profile_removal_still_removes_the_index_service() {
        let steps = Arc::new(Mutex::new(Vec::new()));
        let profile = steps.clone();
        before_uninstall_with(
            move || {
                profile.lock().unwrap().push("profile");
                Err(0x8007_00B7_u32 as i32)
            },
            || steps.lock().unwrap().push("service"),
            Duration::from_secs(1),
        );
        assert_eq!(*steps.lock().unwrap(), ["profile", "service"]);
    }

    #[test]
    fn a_stalled_profile_removal_still_removes_the_index_service_in_time() {
        let (release, stalled) = mpsc::channel::<()>();
        let served = Arc::new(AtomicBool::new(false));
        let seen = served.clone();
        let started = std::time::Instant::now();
        before_uninstall_with(
            move || {
                let _ = stalled.recv_timeout(Duration::from_secs(3));
                Ok(())
            },
            move || seen.store(true, Ordering::SeqCst),
            Duration::from_millis(100),
        );
        assert!(served.load(Ordering::SeqCst));
        assert!(started.elapsed() < Duration::from_secs(1));
        drop(release);
    }

    #[test]
    fn the_elevated_deadline_is_inside_the_uninstall_budget() {
        assert!(navo_index_service::REMOVE_DEADLINE < REMOVE_BUDGET);
    }

    #[test]
    fn both_waits_fit_inside_the_velopack_hook_timeout() {
        assert!(PROFILE_BUDGET + REMOVE_BUDGET < Duration::from_secs(45));
    }
}
