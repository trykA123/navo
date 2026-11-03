use navo_ui::window_view::ServiceHooks;

#[cfg(any(windows, test))]
use crate::index_service::ElevationError;
#[cfg(any(windows, test))]
use navo_ui::window_view::{ServiceOutcome, ServiceStatus};

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

pub(crate) fn add_upgrade_notice(service: &mut ServiceHooks, dirs: Option<&navo_store::StoreDirs>) {
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
    use crate::index_service::is_older;
    use navo_index::pipe::SERVICE_REVISION;

    let Ok(query) = query else {
        return ServiceStatus::NotAvailable;
    };
    let is_running_older = query
        .version
        .as_ref()
        .is_some_and(|version| is_older(version, SERVICE_REVISION));
    match query.running {
        None => ServiceStatus::NotInstalled,
        Some(true) if is_running_older => ServiceStatus::Outdated,
        Some(true) => ServiceStatus::Running,
        Some(false) => ServiceStatus::Stopped,
    }
}

#[cfg(any(windows, test))]
fn action_outcome(result: Result<(), ElevationError>) -> Result<(), ElevationError> {
    match result {
        Err(ElevationError::Cancelled) => Ok(()),
        result => result,
    }
}

#[cfg(any(windows, test))]
fn report(
    action: crate::index_service::Action,
    result: Result<(), ElevationError>,
) -> ServiceOutcome {
    match action_outcome(result) {
        Ok(()) => ServiceOutcome::Done,
        Err(error) => {
            tracing::warn!(?action, %error, "index service change failed");
            ServiceOutcome::Failed
        }
    }
}

#[cfg(windows)]
fn elevate(action: crate::index_service::Action) -> ServiceOutcome {
    report(action, crate::index_service::request_elevation(action))
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
                    version: None,
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
    fn a_running_service_that_answers_older_is_outdated_not_running() {
        use navo_index::pipe::{SERVICE_REVISION, ServiceVersion};

        let query = |version| {
            Ok(crate::index_service::ServiceQuery {
                running: Some(true),
                version: Some(version),
                legacy_present: false,
            })
        };
        let known = |revision| ServiceVersion::Known {
            revision,
            build: "test".into(),
        };
        assert_eq!(
            status_from_query(query(ServiceVersion::Older)),
            ServiceStatus::Outdated
        );
        assert_eq!(
            status_from_query(query(known(SERVICE_REVISION - 1))),
            ServiceStatus::Outdated
        );
        assert_eq!(
            status_from_query(query(known(SERVICE_REVISION))),
            ServiceStatus::Running
        );
        assert!(ServiceStatus::Outdated.can_install());
    }

    #[test]
    fn a_failed_change_is_reported_and_a_cancelled_one_is_not() {
        use crate::index_service::Action;

        let failed = report(Action::Install, Err(ElevationError::Failed(3)));
        assert_eq!(failed, ServiceOutcome::Failed);
        let cancelled = report(Action::Install, Err(ElevationError::Cancelled));
        assert_eq!(cancelled, ServiceOutcome::Done);
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
            navo_store::JobJournal::new(navo_store::Db::open_in_memory().unwrap()).unwrap(),
        ));
        for db in [None, Some(db)] {
            let hooks = super::super::hooks(db, super::super::Remember::default());
            let service = hooks.service;
            assert_eq!((service.status)(), ServiceStatus::NotAvailable);
            assert!(!(service.status)().can_install());
            assert!(!(service.status)().can_remove());
            assert_eq!((service.install)(), ServiceOutcome::Done);
            assert_eq!((service.remove)(), ServiceOutcome::Done);
            assert_eq!((service.status)(), ServiceStatus::NotAvailable);
        }
    }
}
