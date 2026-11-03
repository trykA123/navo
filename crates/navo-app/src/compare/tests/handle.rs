use std::io::Read;
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use navo_compare::model::CaseRule;
use navo_compare::model::{
    CancelToken, ContentKey, Generation, HashCache, ObjectId, ReadHint, Reason, RelPath, Row,
    SideEntry, SideSource, Stamp, Status,
};
use navo_compare::rules::{IgnoreRules, IgnoreSource};
use navo_compare::session::{CheckMode, Session, SessionOptions, Totals};
use navo_core::{FileIdentity, ProviderId, ProviderPath};

use super::{location, write};
use crate::compare::handle::{
    CompareError, CompareEvent, CompareHandle, CompareStart, CompareTarget, Opened,
};
use crate::compare::settings::CompareSettings;

const WAIT: Duration = Duration::from_secs(20);

struct NoCache;

impl HashCache for NoCache {
    fn get(&self, _key: &ContentKey) -> Option<[u8; 32]> {
        None
    }

    fn put(&self, _key: &ContentKey, _hash: [u8; 32]) {}
}

fn start(left: &Path, right: &Path) -> (CompareHandle, Receiver<CompareEvent>) {
    let (sender, receiver) = mpsc::channel();
    let request = CompareStart {
        left: CompareTarget::Local(location(left)),
        right: CompareTarget::Local(location(right)),
        settings: CompareSettings::default(),
        cache: Arc::new(NoCache),
    };
    (CompareHandle::start(request, sender), receiver)
}

/// Rows and the final totals of the generation that finishes first.
pub(super) fn finish(receiver: &Receiver<CompareEvent>) -> (Vec<Row>, Totals, Generation) {
    let mut rows = Vec::new();
    let mut last = Totals::default();
    let mut last_generation = Generation::default();
    loop {
        match receiver.recv_timeout(WAIT).unwrap() {
            CompareEvent::Rows(batch) => rows.extend(batch.rows),
            CompareEvent::Totals { generation, totals } => {
                assert!(generation >= last_generation);
                last_generation = generation;
                last = totals;
            }
            CompareEvent::Finished { generation } => return (rows, last, generation),
            CompareEvent::Failed(error) => panic!("compare failed: {error:?}"),
        }
    }
}

pub(super) fn final_status(rows: &[Row], name: &str) -> Status {
    rows.iter()
        .filter(|row| {
            let entry = row.left.as_ref().or(row.right.as_ref());
            entry.is_some_and(|entry| entry.rel.as_path().to_text() == name)
        })
        .map(|row| row.raw_status)
        .next_back()
        .unwrap()
}

fn two_trees() -> (tempfile::TempDir, tempfile::TempDir) {
    let (left, right) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    for (root, other) in [(left.path(), "left only"), (right.path(), "right only")] {
        write(root, "same.txt", b"same bytes");
        write(root, "sub/inner.txt", b"x");
        write(
            root,
            "diff.txt",
            if other.starts_with('l') {
                b"aaa"
            } else {
                b"bbb"
            },
        );
        write(root, &format!("{}.txt", other.replace(' ', "_")), b"o");
    }
    (left, right)
}

#[test]
fn two_local_trees_give_the_expected_rows_and_totals() {
    let (left, right) = two_trees();
    let (_handle, receiver) = start(left.path(), right.path());
    let (rows, totals, _) = finish(&receiver);
    assert_eq!(final_status(&rows, "same.txt"), Status::Same);
    assert_eq!(final_status(&rows, "sub/inner.txt"), Status::Same);
    assert_eq!(final_status(&rows, "left_only.txt"), Status::OnlyLeft);
    assert_eq!(final_status(&rows, "right_only.txt"), Status::OnlyRight);
    assert!(matches!(
        final_status(&rows, "diff.txt"),
        Status::Different { .. }
    ));
    let counts = (totals.rows, totals.same, totals.different, totals.checking);
    assert_eq!(counts, (6, 3, 1, 0));
    assert_eq!((totals.only_left, totals.only_right), (1, 1));
}

#[test]
fn refresh_runs_the_compare_again_under_a_new_generation() {
    let (left, right) = two_trees();
    let (handle, receiver) = start(left.path(), right.path());
    let (_, _, first) = finish(&receiver);
    handle.refresh();
    let (_, totals, second) = finish(&receiver);
    assert!(second > first);
    assert_eq!(totals.rows, 6);
}

#[test]
fn focus_and_stop_commands_leave_every_row_in_the_totals() {
    let (left, right) = two_trees();
    let (handle, receiver) = start(left.path(), right.path());
    handle.focus(vec![0, 1]);
    handle.stop_row(0);
    handle.stop_all();
    let (_, totals, _) = finish(&receiver);
    assert_eq!(totals.rows, 6);
}

#[test]
fn a_missing_root_fails_on_its_side() {
    let right = tempfile::tempdir().unwrap();
    let missing = right.path().join("gone");
    let (_handle, receiver) = start(&missing, right.path());
    match receiver.recv_timeout(WAIT).unwrap() {
        CompareEvent::Failed(error) => assert_eq!(error, CompareError::Left(Reason::NotFound)),
        other => panic!("expected a failure, got {other:?}"),
    }
}

/// How the test source lists.
#[derive(Clone, Copy)]
enum Listing {
    /// Waits for cancel, up to 30 s.
    UntilCancelled,
    /// Ignores cancel for this long.
    Stubborn(Duration),
    /// Returns at once with no entries.
    Instant,
}

struct Sleepy {
    provider: ProviderId,
    listing: Listing,
    /// Set when a stubborn list has returned.
    listed: Arc<AtomicBool>,
    /// Set when the source is dropped, which happens as the session thread ends.
    gone: Arc<AtomicBool>,
}

impl Drop for Sleepy {
    fn drop(&mut self) {
        self.gone.store(true, Ordering::SeqCst);
    }
}

impl SideSource for Sleepy {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn list(
        &self,
        _folder: &RelPath,
        _sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        cancel: &CancelToken,
    ) -> Result<(), Reason> {
        match self.listing {
            Listing::UntilCancelled => {
                let until = Instant::now() + Duration::from_secs(30);
                while !cancel.is_cancelled() && Instant::now() < until {
                    thread::sleep(Duration::from_millis(5));
                }
            }
            Listing::Stubborn(hold) => {
                thread::sleep(hold);
                self.listed.store(true, Ordering::SeqCst);
            }
            Listing::Instant => {}
        }
        Ok(())
    }

    fn open(&self, _rel: &RelPath, _hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        Err(Reason::NotFound)
    }

    fn stamp(&self, _rel: &RelPath) -> Result<Stamp, Reason> {
        Err(Reason::NotFound)
    }

    fn object_id(&self, _rel: &RelPath) -> Option<ObjectId> {
        None
    }

    fn file_id(&self, _rel: &RelPath) -> Option<FileIdentity> {
        None
    }

    fn cache_path(&self, rel: &RelPath) -> ProviderPath {
        rel.as_path().clone()
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        false
    }
}

struct Probe {
    handle: CompareHandle,
    receiver: Receiver<CompareEvent>,
    listed: Arc<AtomicBool>,
    gone: Arc<AtomicBool>,
}

fn sleepy_handle(listing: Listing) -> Probe {
    let (sender, receiver) = mpsc::channel();
    let (listed, gone) = (Arc::default(), Arc::default());
    let flags = (Arc::clone(&listed), Arc::clone(&gone));
    let open = move || {
        let rules = IgnoreRules::new(Vec::<IgnoreSource>::new(), &[], false, CaseRule::Sensitive)
            .map_err(|error| CompareError::Rules(error.to_string()))?
            .0;
        let side = || -> Arc<dyn SideSource> {
            Arc::new(Sleepy {
                provider: ProviderId::local(),
                listing,
                listed: Arc::clone(&flags.0),
                gone: Arc::clone(&flags.1),
            })
        };
        let options = SessionOptions {
            rules: Arc::new(rules),
            cache: Arc::new(NoCache),
            read_hint: ReadHint::Buffered,
            check_mode: CheckMode::Content,
        };
        Ok(Opened {
            session: Session::start(side(), side(), options),
            git_sides: Vec::new(),
        })
    };
    let handle = CompareHandle::spawn(open, sender);
    Probe {
        handle,
        receiver,
        listed,
        gone,
    }
}

fn thread_ends(receiver: &Receiver<CompareEvent>) -> bool {
    let until = Instant::now() + Duration::from_secs(10);
    while Instant::now() < until {
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Err(RecvTimeoutError::Disconnected) => return true,
            Ok(_) | Err(RecvTimeoutError::Timeout) => {}
        }
    }
    false
}

#[test]
fn dropping_the_handle_returns_at_once_and_ends_the_thread() {
    let probe = sleepy_handle(Listing::Stubborn(Duration::from_millis(600)));
    thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    drop(probe.handle);
    assert!(started.elapsed() < Duration::from_millis(100));
    assert!(
        !probe.listed.load(Ordering::SeqCst),
        "list ended before the drop"
    );
    assert!(thread_ends(&probe.receiver));
    assert!(
        probe.listed.load(Ordering::SeqCst),
        "the thread ended while a list was still running"
    );
}

#[test]
fn cancel_ends_the_thread_while_a_listing_is_running() {
    let probe = sleepy_handle(Listing::UntilCancelled);
    thread::sleep(Duration::from_millis(100));
    probe.handle.cancel();
    assert!(thread_ends(&probe.receiver));
    drop(probe.handle);
}

#[test]
fn a_dropped_receiver_ends_the_thread() {
    let probe = sleepy_handle(Listing::Instant);
    loop {
        if let CompareEvent::Finished { .. } = probe.receiver.recv_timeout(WAIT).unwrap() {
            break;
        }
    }
    drop(probe.receiver);
    probe.handle.refresh();
    let until = Instant::now() + Duration::from_secs(10);
    while !probe.gone.load(Ordering::SeqCst) && Instant::now() < until {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        probe.gone.load(Ordering::SeqCst),
        "the session thread kept running with no receiver"
    );
}
