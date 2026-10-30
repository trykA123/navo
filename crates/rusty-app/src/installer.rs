#[cfg(any(windows, test))]
use std::io;
#[cfg(any(windows, test))]
use std::sync::mpsc;
#[cfg(any(windows, test))]
use std::time::Duration;

#[cfg(windows)]
use crate::index_service;
#[cfg(windows)]
use crate::logging;
#[cfg(windows)]
use rusty_store::StoreDirs;

#[cfg(windows)]
const REMOVE_BUDGET: Duration = Duration::from_secs(25);

pub(crate) fn run() {
    let mut app = velopack::VelopackApp::build();
    #[cfg(windows)]
    {
        app = app.on_before_uninstall_fast_callback(|_| remove_index_service());
    }
    app.run();
}

#[cfg(any(windows, test))]
#[derive(Debug, PartialEq, Eq)]
enum Removal {
    NotInstalled,
    Removed,
    Skipped(String),
}

#[cfg(any(windows, test))]
fn remove_when_installed(
    installed: impl FnOnce() -> io::Result<bool>,
    remove: impl FnOnce() -> Result<(), String> + Send + 'static,
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
        Ok(Ok(())) => Removal::Removed,
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
                Ok(())
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
                Ok(())
            },
            Duration::from_millis(100),
        );
        assert!(matches!(outcome, Removal::Skipped(_)));
    }

    #[test]
    fn a_failed_query_skips_the_removal() {
        let outcome = remove_when_installed(
            || Err(io::ErrorKind::PermissionDenied.into()),
            || Ok(()),
            Duration::from_secs(1),
        );
        assert!(matches!(outcome, Removal::Skipped(_)));
    }
}
