use std::sync::Arc;
use std::time::{Duration, Instant};

use navo_core::Location;
use navo_index::pipe::{Client, FolderTotalsReply};
use navo_ui::browser::{FolderTotalsQuery, FolderTotalsSource, TotalsState};

fn service_reply(paths: &[String]) -> Option<FolderTotalsReply> {
    Client::folder_totals_reply(paths, Instant::now() + Duration::from_millis(40)).ok()
}

/// The index service as a totals source; the reply function is injected so tests need no pipe.
struct ServiceTotals {
    reply: fn(&[String]) -> Option<FolderTotalsReply>,
}

impl FolderTotalsSource for ServiceTotals {
    fn query(&self, folder: &Location) -> TotalsState {
        if !folder.is_local() {
            return TotalsState::Unavailable;
        }
        let Some(reply) = (self.reply)(&[folder.as_str().to_owned()]) else {
            return TotalsState::Unavailable;
        };
        match reply.totals.into_iter().next().flatten() {
            Some(totals) => TotalsState::Ready(totals),
            None if reply.busy => TotalsState::Busy,
            None => TotalsState::Unavailable,
        }
    }
}

pub fn query() -> FolderTotalsQuery {
    Arc::new(ServiceTotals {
        reply: service_reply,
    })
}

#[cfg(test)]
mod tests {
    use navo_index::pipe::FolderTotals;

    use super::*;

    fn folder() -> Location {
        Location::new(r"C:\proj").unwrap()
    }

    fn totals() -> FolderTotals {
        FolderTotals {
            bytes: 4096,
            files: 3,
            folders: 1,
            complete: true,
        }
    }

    fn rebuilding(_: &[String]) -> Option<FolderTotalsReply> {
        Some(FolderTotalsReply {
            totals: vec![None],
            busy: true,
        })
    }

    fn expired_deadline(_: &[String]) -> Option<FolderTotalsReply> {
        Some(FolderTotalsReply {
            totals: vec![None],
            busy: false,
        })
    }

    fn complete_reply(paths: &[String]) -> Option<FolderTotalsReply> {
        assert_eq!(
            paths,
            [r"C:\proj".to_owned()].as_slice(),
            "the source asks for the one folder it was given"
        );
        Some(FolderTotalsReply {
            totals: vec![Some(totals())],
            busy: false,
        })
    }

    fn transport_failure(_: &[String]) -> Option<FolderTotalsReply> {
        None
    }

    fn never_asked(_: &[String]) -> Option<FolderTotalsReply> {
        panic!("a non-local folder must not reach the index service");
    }

    #[test]
    fn rebuilding_reply_stays_busy() {
        assert_eq!(
            ServiceTotals { reply: rebuilding }.query(&folder()),
            TotalsState::Busy,
            "a rebuilding volume must stay Busy so a row waits instead of falling back"
        );
    }

    #[test]
    fn matching_total_is_ready() {
        assert_eq!(
            ServiceTotals {
                reply: complete_reply
            }
            .query(&folder()),
            TotalsState::Ready(totals())
        );
    }

    #[test]
    fn expired_deadline_reply_is_unavailable() {
        assert_eq!(
            ServiceTotals {
                reply: expired_deadline
            }
            .query(&folder()),
            TotalsState::Unavailable,
            "a service deadline expiry must not leave the row waiting forever"
        );
    }

    #[test]
    fn transport_failure_is_unavailable() {
        assert_eq!(
            ServiceTotals {
                reply: transport_failure
            }
            .query(&folder()),
            TotalsState::Unavailable
        );
    }

    #[test]
    fn non_local_folder_is_unavailable_without_asking() {
        let network = Location::new("ftp://host/pub").unwrap();
        assert_eq!(
            ServiceTotals { reply: never_asked }.query(&network),
            TotalsState::Unavailable
        );
    }

    #[test]
    fn oversized_request_returns_none_before_connecting() {
        assert!(super::service_reply(&vec!["C:".into(); 65]).is_none());
    }
    #[cfg(not(windows))]
    #[test]
    fn absent_service_returns_none_within_budget() {
        let started = std::time::Instant::now();
        assert!(super::service_reply(&["C:\\folder".into()]).is_none());
        assert!(started.elapsed() < std::time::Duration::from_millis(50));
    }
}
