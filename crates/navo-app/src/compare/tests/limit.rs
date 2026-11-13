//! The network limit through the handle, and the drive classification of a local side.

use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

use navo_compare::model::{CaseRule, ReadHint, SideSource};
use navo_compare::rules::{IgnoreRules, IgnoreSource};
use navo_compare::session::{CheckMode, NetworkCap, Session, SessionOptions};
use navo_fs::DriveClass;

use super::handle::{NoCache, two_trees};
use super::location;
use crate::compare::handle::{CompareEvent, CompareHandle, Opened};
use crate::compare::local::LocalSide;
use crate::compare::settings::network_cap;

const WAIT: Duration = Duration::from_secs(20);

fn side(root: &std::path::Path, class: DriveClass) -> Arc<dyn SideSource> {
    Arc::new(LocalSide::with_class(location(root), |_| class).unwrap())
}

#[test]
fn only_a_network_class_makes_a_side_network() {
    let dir = tempfile::tempdir().unwrap();
    assert!(side(dir.path(), DriveClass::Network).is_network());
    assert!(!side(dir.path(), DriveClass::Ssd).is_network());
}

#[test]
fn an_unknown_class_counts_as_not_network() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!side(dir.path(), DriveClass::Unknown).is_network());
}

/// An open function for a one-entry cap that never stops by time.
fn network_open(
    left_path: std::path::PathBuf,
    right_path: std::path::PathBuf,
) -> impl FnOnce() -> Result<Opened, crate::compare::handle::CompareError> + Send + 'static {
    move || {
        let rules = IgnoreRules::new(Vec::<IgnoreSource>::new(), &[], false, CaseRule::Sensitive)
            .unwrap()
            .0;
        let options = SessionOptions {
            rules: Arc::new(rules),
            cache: Arc::new(NoCache),
            read_hint: ReadHint::Buffered,
            check_mode: CheckMode::SizeTime,
            network_cap: Some(NetworkCap {
                entries: 1,
                duration: Duration::from_secs(600),
            }),
        };
        let session = Session::start(
            side(&left_path, DriveClass::Network),
            side(&right_path, DriveClass::Ssd),
            options,
        );
        Ok(Opened {
            session,
            git_sides: Vec::new(),
        })
    }
}

#[test]
fn a_network_walk_reports_each_stop_and_continues_to_the_end() {
    let (left, right) = two_trees();
    let (sender, receiver) = mpsc::channel();
    let open = network_open(left.path().to_owned(), right.path().to_owned());
    let handle = CompareHandle::spawn(open, sender);
    let (mut stops, mut continues, mut rows) = (0, 0, 0);
    loop {
        match receiver.recv_timeout(WAIT).unwrap() {
            CompareEvent::StoppedAtLimit { entries, .. } => {
                assert!(entries > 0);
                stops += 1;
                handle.continue_walk();
            }
            CompareEvent::Continued { .. } => continues += 1,
            CompareEvent::Totals { totals, .. } => rows = totals.rows,
            CompareEvent::Finished { .. } => break,
            CompareEvent::Failed(error) => panic!("compare failed: {error:?}"),
            _ => {}
        }
    }
    assert!(stops >= 1);
    assert_eq!(stops, continues);
    assert_eq!(rows, 6);
}

#[test]
fn the_cap_given_to_the_engine_is_the_shared_navo_fs_bound() {
    let cap = network_cap();
    assert_eq!(cap.entries, navo_fs::NETWORK_WALK_MAX_ENTRIES);
    assert_eq!(cap.duration, navo_fs::NETWORK_WALK_MAX_TIME);
    assert_eq!(
        (cap.entries, cap.duration),
        (200_000, Duration::from_secs(60))
    );
}

#[test]
fn a_refresh_after_a_stop_does_not_report_a_continue() {
    let (left, right) = two_trees();
    let (sender, receiver) = mpsc::channel();
    let open = network_open(left.path().to_owned(), right.path().to_owned());
    let handle = CompareHandle::spawn(open, sender);
    let mut limit_events = Vec::new();
    loop {
        match receiver.recv_timeout(WAIT).unwrap() {
            CompareEvent::StoppedAtLimit { generation, .. } => {
                limit_events.push(("stopped", generation.get()));
                if limit_events.len() >= 2 {
                    break;
                }
                handle.refresh();
            }
            CompareEvent::Continued { generation } => {
                limit_events.push(("continued", generation.get()));
            }
            CompareEvent::Failed(error) => panic!("compare failed: {error:?}"),
            _ => {}
        }
    }
    let first = limit_events.first().map(|event| event.1).unwrap();
    assert_eq!(
        limit_events,
        vec![("stopped", first), ("stopped", first + 1)]
    );
}
