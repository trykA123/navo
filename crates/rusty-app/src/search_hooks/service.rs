use rusty_ui::window_view::ServiceHooks;

#[cfg(any(windows, test))]
use crate::index_service::ElevationError;
#[cfg(any(windows, test))]
use rusty_ui::window_view::ServiceStatus;

pub(super) fn hooks() -> ServiceHooks {
    #[cfg(windows)]
    {
        use std::sync::Arc;

        use crate::index_service::{Action, query};

        ServiceHooks {
            status: Arc::new(|| status_from_query(query())),
            install: Arc::new(|| elevate(Action::Install)),
            remove: Arc::new(|| elevate(Action::Remove)),
            ..ServiceHooks::none()
        }
    }
    #[cfg(not(windows))]
    ServiceHooks::none()
}

pub(crate) fn add_upgrade_notice(
    service: &mut ServiceHooks,
    dirs: Option<&rusty_store::StoreDirs>,
) {
    #[cfg(windows)]
    {
        use std::sync::Arc;

        let marker = dirs.map(|dirs| dirs.data.join(UPGRADE_DISMISSED));
        let dismissed = marker.clone();
        service.upgrade_due =
            Arc::new(move || crate::index_service::upgrade_notice_due(marker.clone()));
        service.dismiss_upgrade = Arc::new(move || {
            if let Err(error) = crate::index_service::dismiss_upgrade_notice(dismissed.clone()) {
                tracing::warn!(%error, "cannot remember the dismissed index service notice");
            }
        });
    }
    #[cfg(not(windows))]
    let _ = (service, dirs);
}

#[cfg(windows)]
const UPGRADE_DISMISSED: &str = "index-upgrade-dismissed";

#[cfg(any(windows, test))]
fn status_from_query(query: std::io::Result<crate::index_service::ServiceQuery>) -> ServiceStatus {
    match query.map(|query| query.running) {
        Ok(None) => ServiceStatus::NotInstalled,
        Ok(Some(true)) => ServiceStatus::Running,
        Ok(Some(false)) => ServiceStatus::Stopped,
        Err(_) => ServiceStatus::NotAvailable,
    }
}

#[cfg(any(windows, test))]
fn action_outcome(result: Result<(), ElevationError>) -> Result<(), ElevationError> {
    match result {
        Err(ElevationError::Cancelled) => Ok(()),
        result => result,
    }
}

#[cfg(windows)]
fn elevate(action: crate::index_service::Action) {
    if let Err(error) = action_outcome(crate::index_service::request_elevation(action)) {
        eprintln!("navo: index service: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queried_service_presence_and_state_determine_status() {
        for legacy_present in [false, true] {
            let query = |running| {
                Ok(crate::index_service::ServiceQuery {
                    running,
                    legacy_present,
                })
            };
            assert_eq!(status_from_query(query(None)), ServiceStatus::NotInstalled);
            assert_eq!(status_from_query(query(Some(true))), ServiceStatus::Running);
            assert_eq!(
                status_from_query(query(Some(false))),
                ServiceStatus::Stopped
            );
        }
    }

    #[test]
    fn an_unreadable_scm_is_unavailable_and_disables_actions() {
        let status = status_from_query(Err(std::io::Error::from(
            std::io::ErrorKind::PermissionDenied,
        )));
        assert_eq!(status, ServiceStatus::NotAvailable);
        assert!(!status.can_install() && !status.can_remove());
    }

    #[test]
    fn cancelling_elevation_has_the_same_outcome_as_no_change() {
        assert!(action_outcome(Err(ElevationError::Cancelled)).is_ok());
        assert!(action_outcome(Ok(())).is_ok());
    }

    #[test]
    fn operation_and_launch_failures_are_preserved() {
        assert!(matches!(
            action_outcome(Err(ElevationError::Failed(7))),
            Err(ElevationError::Failed(7))
        ));
        assert!(matches!(
            action_outcome(Err(std::io::Error::other("launch failed").into())),
            Err(ElevationError::Launch(_))
        ));
    }

    #[cfg(not(windows))]
    #[test]
    fn non_windows_hooks_are_unavailable_and_actions_stay_disabled() {
        let db = std::sync::Arc::new(std::sync::Mutex::new(
            rusty_store::JobJournal::new(rusty_store::Db::open_in_memory().unwrap()).unwrap(),
        ));
        for db in [None, Some(db)] {
            let hooks = super::super::hooks(db, super::super::Remember::default());
            let service = hooks.service;
            assert_eq!((service.status)(), ServiceStatus::NotAvailable);
            assert!(!(service.status)().can_install());
            assert!(!(service.status)().can_remove());
            (service.install)();
            (service.remove)();
            assert_eq!((service.status)(), ServiceStatus::NotAvailable);
        }
    }
}
