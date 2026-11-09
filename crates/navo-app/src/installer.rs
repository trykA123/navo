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
use navo_index_service::Elevated;
#[cfg(windows)]
use navo_store::StoreDirs;

/// The uninstall callback's wait for the elevated remove, which has `REMOVE_DEADLINE`.
#[cfg(any(windows, test))]
const REMOVE_BUDGET: Duration = Duration::from_secs(25);

static RESTARTED: AtomicBool = AtomicBool::new(false);

/// Runs Velopack's start-up hooks. Staged packages apply only through `updater::apply`.
pub(crate) fn run() {
    let mut app = velopack::VelopackApp::build()
        .set_auto_apply_on_startup(false)
        .on_restarted(|_| RESTARTED.store(true, Ordering::Relaxed));
    #[cfg(windows)]
    {
        app = app.on_before_uninstall_fast_callback(|_| remove_index_service());
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
    let (done, finished) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(remove());
    });
    match finished.recv_timeout(wait) {
        Ok(Ok(Elevated::Done)) => Removal::Removed,
        Ok(Ok(Elevated::Deferred)) => Removal::Deferred,
        Ok(Err(error)) => Removal::Skipped(error),
        Err(_) => Removal::Skipped(format!("no answer within {} s", wait.as_secs())),
    }
}

#[cfg(windows)]
fn remove_index_service() {
    let dirs = StoreDirs::detect();
    let _diagnostics = logging::install(dirs.as_ref().map(|dirs| dirs.data.as_path()));
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
        Removal::NotInstalled => tracing::info!("uninstall: no index service to remove"),
        Removal::Removed => tracing::info!("uninstall: index service removed"),
        Removal::Deferred => {
            tracing::info!(
                "uninstall: index service removed; its last files go at the next restart"
            );
        }
        Removal::Skipped(reason) => {
            tracing::warn!(reason, "uninstall: index service left in place");
        }
    }
    logging::shutdown();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

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
    fn the_elevated_deadline_is_inside_the_uninstall_budget() {
        assert!(navo_index_service::REMOVE_DEADLINE < REMOVE_BUDGET);
    }
}
