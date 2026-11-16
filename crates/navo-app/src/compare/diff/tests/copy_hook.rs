//! Tests for copying one change through the DiffHooks wiring (V27): a backend with no job
//! factory refuses, and a real temp-file copy writes the right file and reports Copied.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use navo_core::{Location, ProviderPath};
use navo_fs::LocalFs;
use navo_store::StoreDirs;
use navo_ui::compare_diff::{
    CompareOptions, CopyChange, CopyDirection, CopyDone, CopyResult, CopySink, DiffHooks, DiffSink,
    DiffUpdate, PairRequest, SheetPlan, SheetPlanSink, SheetResult, SheetSink,
};

use crate::compare::diff::copy::CopyRefusal;
use crate::compare::diff::copy_hook::navo_copy_backend;
use crate::compare::diff::copy_port::{refuse_change, refuse_sheet};
use crate::compare::diff::{CopyHost, DiffBackend};
use crate::jobs::{SessionHolds, open};

const WAIT: Duration = Duration::from_secs(20);

fn location(path: &Path) -> Location {
    Location::local(ProviderPath::from_text(path.to_str().unwrap_or_default())).unwrap()
}

fn write(root: &Path, name: &str, bytes: &[u8]) {
    fs::write(root.join(name), bytes).unwrap();
}

fn sink() -> (DiffSink, Receiver<DiffUpdate>) {
    let (sender, receiver) = mpsc::channel();
    let sink: DiffSink = Arc::new(move |update| {
        let _ = sender.send(update);
    });
    (sink, receiver)
}

fn request(id: u64, left: &Path, right: &Path) -> PairRequest {
    PairRequest {
        id,
        path: "left.txt".to_owned(),
        left: Some(location(left)),
        right: Some(location(right)),
    }
}

/// The sink of a copy and the receiver of its end.
fn copy_sink() -> (CopySink, Receiver<CopyDone>) {
    let (sender, receiver) = mpsc::channel();
    let sink: CopySink = Arc::new(move |done| {
        let _ = sender.send(done);
    });
    (sink, receiver)
}

/// Opens the pair and returns the change the UI would send for each hunk.
fn all_changes(backend: &DiffBackend, left: &Path, right: &Path) -> Vec<CopyChange> {
    let (sink, receiver) = sink();
    let pair = request(9, left, right);
    let _control = backend.open_pair(pair.clone(), &CompareOptions::default(), sink);
    let DiffUpdate::Opened { text, .. } = receiver.recv_timeout(WAIT).unwrap() else {
        panic!("expected Opened");
    };
    let hunks = text.unwrap().hunks.clone();
    hunks
        .into_iter()
        .map(|hunk| CopyChange {
            pair: pair.clone(),
            left_bytes: hunk.left_bytes,
            right_bytes: hunk.right_bytes,
            digest: hunk.digest.unwrap(),
            direction: CopyDirection::ToRight,
            lines: None,
        })
        .collect()
}

/// The change the UI would send for the first hunk.
fn first_change(backend: &DiffBackend, left: &Path, right: &Path) -> CopyChange {
    all_changes(backend, left, right).remove(0)
}

#[test]
fn a_backend_without_a_job_factory_refuses_a_copy() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "left.txt", b"alpha\nbeta\n");
    write(dir.path(), "right.txt", b"alpha\ngamma\n");
    let backend = DiffBackend::default();
    let change = first_change(
        &backend,
        &dir.path().join("left.txt"),
        &dir.path().join("right.txt"),
    );
    let (done, receiver) = copy_sink();
    backend.copy_change(change, done);
    let end = receiver.recv_timeout(WAIT).unwrap();
    assert_eq!((end.request, end.direction), (9, CopyDirection::ToRight));
    assert!(matches!(end.result, CopyResult::Refused(_)));
}

#[test]
fn a_copy_through_the_backend_writes_the_right_file_and_reports_copied() {
    let dir = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    write(dir.path(), "left.txt", b"alpha\nbeta\n");
    write(dir.path(), "right.txt", b"alpha\ngamma\n");
    let dirs = StoreDirs::in_profile(store.path());
    dirs.ensure().unwrap();
    let journal = open(&dirs.journal_file()).unwrap();
    let factory = SessionHolds::new(journal, dirs).compare_factory(None);
    let backend = navo_copy_backend(factory, Arc::new(LocalFs::new()));
    let change = first_change(
        &backend,
        &dir.path().join("left.txt"),
        &dir.path().join("right.txt"),
    );
    let (done, receiver) = copy_sink();
    backend.copy_change(change, done);
    let end = receiver.recv_timeout(WAIT).unwrap();
    assert_eq!(end.result, CopyResult::Copied);
    assert_eq!(
        fs::read(dir.path().join("right.txt")).unwrap(),
        b"alpha\nbeta\n"
    );
}

#[test]
fn a_backend_without_a_job_factory_plans_nothing_and_refuses_a_sheet_run() {
    let backend = DiffBackend::default();
    let (planned, plans) = mpsc::channel();
    backend.plan_sheet(Vec::new(), Arc::new(move |plan| drop(planned.send(plan))));
    assert_eq!(plans.recv_timeout(WAIT).unwrap(), SheetPlan::default());
    let (sender, results) = mpsc::channel();
    backend.run_sheet(Arc::new(move |result| drop(sender.send(result))));
    assert!(matches!(
        results.recv_timeout(WAIT).unwrap(),
        SheetResult::Refused(_)
    ));
}

#[test]
fn a_sheet_through_the_backend_plans_then_writes_once() {
    let dir = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    write(dir.path(), "left.txt", b"alpha\nbeta\n");
    write(dir.path(), "right.txt", b"alpha\ngamma\n");
    let dirs = StoreDirs::in_profile(store.path());
    dirs.ensure().unwrap();
    let journal = open(&dirs.journal_file()).unwrap();
    let factory = SessionHolds::new(journal, dirs).compare_factory(None);
    let backend = navo_copy_backend(factory, Arc::new(LocalFs::new()));
    let change = first_change(
        &backend,
        &dir.path().join("left.txt"),
        &dir.path().join("right.txt"),
    );
    let (planned, plans) = mpsc::channel();
    backend.plan_sheet(vec![change], Arc::new(move |plan| drop(planned.send(plan))));
    let plan = plans.recv_timeout(WAIT).unwrap();
    assert_eq!(plan.rows.len(), 1);
    assert_eq!((plan.rows[0].hunks, plan.rows[0].refusal), (1, None));
    assert_eq!(
        fs::read(dir.path().join("right.txt")).unwrap(),
        b"alpha\ngamma\n",
        "the plan writes nothing"
    );
    let (sender, results) = mpsc::channel();
    let done: navo_ui::compare_diff::SheetSink = Arc::new(move |result| drop(sender.send(result)));
    backend.run_sheet(done.clone());
    let copied = results.recv_timeout(WAIT).unwrap();
    assert!(matches!(copied, SheetResult::Copied { files: 1, .. }));
    assert_eq!(
        fs::read(dir.path().join("right.txt")).unwrap(),
        b"alpha\nbeta\n"
    );
    backend.run_sheet(done);
    assert!(matches!(
        results.recv_timeout(WAIT).unwrap(),
        SheetResult::Refused(_)
    ));
}

#[test]
fn picked_lines_of_two_changes_copy_as_one_job() {
    let dir = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "left.txt",
        b"a\nb1\nb2\nc\nd\ne\nf\ng\nh1\nh2\n",
    );
    write(
        dir.path(),
        "right.txt",
        b"a\nB1\nB2\nc\nd\ne\nf\ng\nH1\nH2\n",
    );
    let dirs = StoreDirs::in_profile(store.path());
    dirs.ensure().unwrap();
    let journal = open(&dirs.journal_file()).unwrap();
    let factory = SessionHolds::new(journal, dirs).compare_factory(None);
    let backend = navo_copy_backend(factory, Arc::new(LocalFs::new()));
    let mut changes = all_changes(
        &backend,
        &dir.path().join("left.txt"),
        &dir.path().join("right.txt"),
    );
    assert_eq!(changes.len(), 2);
    changes[0].lines = Some(BTreeSet::from([2]));
    changes[1].lines = Some(BTreeSet::from([9]));
    let (done, receiver) = copy_sink();
    backend.copy_changes(changes, done);
    let end = receiver.recv_timeout(WAIT).unwrap();
    assert_eq!(end.result, CopyResult::Copied);
    assert_eq!(
        fs::read(dir.path().join("right.txt")).unwrap(),
        b"a\nB1\nb2\nc\nd\ne\nf\ng\nH1\nh2\n"
    );
}

#[test]
fn a_panicking_copy_still_reports_a_refusal() {
    use crate::compare::diff::copy_hook::guarded;
    let (done, ends) = copy_sink();
    let refused = || CopyDone {
        request: 7,
        direction: CopyDirection::ToLeft,
        result: CopyResult::Refused("stopped".to_owned()),
    };
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        guarded(done, refused, || panic!("copy thread died"));
    }));
    assert!(result.is_err());
    let end = ends.try_recv().expect("one end");
    assert_eq!((end.request, end.direction), (7, CopyDirection::ToLeft));
    assert!(matches!(end.result, CopyResult::Refused(_)));
    assert!(ends.try_recv().is_err(), "exactly one end");
}

#[test]
fn a_finished_copy_reports_once() {
    use crate::compare::diff::copy_hook::guarded;
    let (done, ends) = copy_sink();
    let copied = || CopyDone {
        request: 3,
        direction: CopyDirection::ToRight,
        result: CopyResult::Copied,
    };
    guarded(done, || unreachable!("no refusal"), copied);
    assert!(matches!(
        ends.try_recv().unwrap().result,
        CopyResult::Copied
    ));
    assert!(ends.try_recv().is_err());
}

#[test]
fn the_read_only_refusal_matches_the_copy_refusal() {
    let change = CopyChange {
        pair: PairRequest {
            id: 4,
            path: "left.txt".to_owned(),
            left: None,
            right: None,
        },
        left_bytes: 0..1,
        right_bytes: 0..1,
        digest: [0; 16],
        direction: CopyDirection::ToLeft,
        lines: None,
    };
    let (done, receiver) = copy_sink();
    refuse_change(&change, &done);
    let end = receiver.recv_timeout(WAIT).unwrap();
    assert_eq!((end.request, end.direction), (4, CopyDirection::ToLeft));
    let text = CopyRefusal::NotCopyable.text();
    assert_eq!(end.result, CopyResult::Refused(text.clone()));
    let (sender, results) = mpsc::channel();
    refuse_sheet(Arc::new(move |result| drop(sender.send(result))));
    assert_eq!(
        results.recv_timeout(WAIT).unwrap(),
        SheetResult::Refused(text)
    );
}

/// A change of pair `id` with no sides, enough for a backend that never reads the files.
fn bare_change(id: u64) -> CopyChange {
    CopyChange {
        pair: PairRequest {
            id,
            path: "left.txt".to_owned(),
            left: None,
            right: None,
        },
        left_bytes: 0..1,
        right_bytes: 0..1,
        digest: [0; 16],
        direction: CopyDirection::ToRight,
        lines: None,
    }
}

/// A host that records the pair id of every `copy_change` call and writes nothing.
#[derive(Default)]
struct RecordingHost {
    copied: std::sync::Mutex<Vec<u64>>,
}

impl CopyHost for RecordingHost {
    fn copy_change(&self, change: CopyChange, _done: CopySink) {
        self.copied.lock().unwrap().push(change.pair.id);
    }
    fn copy_changes(&self, _changes: Vec<CopyChange>, _done: CopySink) {
        panic!("not called");
    }
    fn plan_sheet(&self, _changes: Vec<CopyChange>, _planned: SheetPlanSink) {
        panic!("not called");
    }
    fn run_sheet(&self, _done: SheetSink) {
        panic!("not called");
    }
}

#[test]
fn a_copy_host_receives_the_change() {
    let host = Arc::new(RecordingHost::default());
    let backend = DiffBackend::default().with_copy_host(host.clone());
    let (done, ends) = copy_sink();
    backend.copy_change(bare_change(5), done);
    assert_eq!(*host.copied.lock().unwrap(), vec![5]);
    assert!(
        ends.try_recv().is_err(),
        "the host answers, not the backend"
    );
}

#[test]
fn a_backend_without_a_host_refuses_every_write() {
    let backend = DiffBackend::default();
    let refused = CopyResult::Refused(CopyRefusal::NotCopyable.text());
    let (done, ends) = copy_sink();
    backend.copy_change(bare_change(1), done.clone());
    backend.copy_changes(vec![bare_change(2), bare_change(3)], done.clone());
    backend.copy_changes(Vec::new(), done);
    let got: Vec<(u64, CopyResult)> = ends
        .try_iter()
        .map(|end| (end.request, end.result))
        .collect();
    assert_eq!(
        got,
        vec![(1, refused.clone()), (2, refused)],
        "first change only"
    );
    let (planned, plans) = mpsc::channel();
    backend.plan_sheet(
        vec![bare_change(4)],
        Arc::new(move |plan| drop(planned.send(plan))),
    );
    assert_eq!(plans.try_recv().unwrap(), SheetPlan::default());
    let (sender, results) = mpsc::channel();
    backend.run_sheet(Arc::new(move |result| drop(sender.send(result))));
    let text = CopyRefusal::NotCopyable.text();
    assert_eq!(results.try_recv().unwrap(), SheetResult::Refused(text));
}
