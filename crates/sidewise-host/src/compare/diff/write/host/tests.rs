//! Tests for `LocalCopyHost` (C3): copies and undos through the real journal in temp folders,
//! the NC167 no-op pick, a panic inside the fill, refusals, and a fault loop over every disk op.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, UNIX_EPOCH};

use navo_core::{Location, ProviderPath};
use navo_fs::durable::{FaultDisk, Plan, RealDisk};
use navo_ui::compare_diff::{
    CompareOptions, CopyChange, CopyDirection, CopyDone, CopyResult, CopySink, DiffHooks, DiffSink,
    DiffUpdate, PairRequest, UndoDone, UndoSink, UndoStart,
};
use sidewise_journal::{Expect, JobKind, Journal};

use super::LocalCopyHost;
use crate::compare::diff::copy_port::CopyHost;
use crate::compare::diff::hooks::DiffBackend;
use crate::compare::diff::write::job::{Committed, copy_between};
use crate::compare::diff::write::request::{CopyRefusal, CopyRequest, ends_of};

const WAIT: Duration = Duration::from_secs(20);
const OLD: &[u8] = b"alpha\ngamma\n";
const NEW: &[u8] = b"alpha\nbeta\n";
/// 2001-09-09, a modified time no test run can reach by writing.
const OLD_MS: u64 = 1_000_000_000_000;

fn location(path: &Path) -> Location {
    Location::local(ProviderPath::from_text(path.to_str().unwrap())).unwrap()
}

/// Two files, an open journal over a `FaultDisk` with `plan`, and a backend with the host.
struct Fixture {
    work: tempfile::TempDir,
    store: tempfile::TempDir,
    disk: Arc<FaultDisk>,
    host: Arc<LocalCopyHost>,
    backend: DiffBackend,
}

impl Fixture {
    fn new(left: &[u8], right: &[u8], plan: Plan) -> Self {
        Self::build(left, right, plan, None)
    }

    fn build(left: &[u8], right: &[u8], plan: Plan, copy: Option<super::CopyFn>) -> Self {
        let work = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        fs::write(work.path().join("left.txt"), left).unwrap();
        fs::write(work.path().join("right.txt"), right).unwrap();
        let old = UNIX_EPOCH + Duration::from_millis(OLD_MS);
        let file = fs::File::options()
            .write(true)
            .open(work.path().join("right.txt"))
            .unwrap();
        file.set_modified(old).unwrap();
        let disk = Arc::new(FaultDisk::new(RealDisk, plan));
        let (journal, _) = Journal::open(store.path(), disk.clone()).unwrap();
        let journal = Arc::new(journal);
        let host = Arc::new(match copy {
            Some(copy) => LocalCopyHost::with_copy(journal, copy),
            None => LocalCopyHost::new(journal),
        });
        let backend = DiffBackend::default().with_copy_host(host.clone());
        Self {
            work,
            store,
            disk,
            host,
            backend,
        }
    }

    fn right_path(&self) -> PathBuf {
        self.work.path().join("right.txt")
    }

    fn right(&self) -> Vec<u8> {
        fs::read(self.right_path()).unwrap()
    }

    fn right_ms(&self) -> u128 {
        let modified = fs::metadata(self.right_path()).unwrap().modified().unwrap();
        modified.duration_since(UNIX_EPOCH).unwrap().as_millis()
    }

    fn pair(&self) -> PairRequest {
        PairRequest {
            id: 9,
            path: "x.txt".to_owned(),
            left: Some(location(&self.work.path().join("left.txt"))),
            right: Some(location(&self.right_path())),
        }
    }

    /// The change the UI would send for each hunk, copied to the right.
    fn changes(&self) -> Vec<CopyChange> {
        let (sender, receiver) = mpsc::channel();
        let sink: DiffSink = Arc::new(move |update| {
            let _ = sender.send(update);
        });
        let pair = self.pair();
        let _control = self
            .backend
            .open_pair(pair.clone(), &CompareOptions::default(), sink);
        let DiffUpdate::Opened { text, .. } = receiver.recv_timeout(WAIT).unwrap() else {
            panic!("expected Opened");
        };
        text.unwrap()
            .hunks
            .iter()
            .map(|hunk| CopyChange {
                pair: pair.clone(),
                left_bytes: hunk.left_bytes.clone(),
                right_bytes: hunk.right_bytes.clone(),
                digest: hunk.digest.unwrap(),
                direction: CopyDirection::ToRight,
                lines: None,
            })
            .collect()
    }

    /// Copy `changes` as one job through the backend and wait for the end.
    fn copy(&self, changes: Vec<CopyChange>) -> CopyResult {
        self.copy_done(changes).result
    }

    /// Undo through the backend; `None` when it had nothing to undo.
    fn undo(&self) -> Option<UndoDone> {
        let (sender, receiver) = mpsc::channel();
        let sink: UndoSink = Arc::new(move |done| {
            let _ = sender.send(done);
        });
        if self.backend.undo_last(None, sink) == UndoStart::Nothing {
            assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());
            return None;
        }
        Some(receiver.recv_timeout(WAIT).unwrap())
    }

    /// The job folders in the journal (everything in `v1` but the owner locks).
    fn jobs(&self) -> usize {
        fs::read_dir(self.store.path().join("v1"))
            .unwrap()
            .filter(|entry| entry.as_ref().unwrap().file_name() != "owners")
            .count()
    }
}

fn copy_sink() -> (CopySink, Receiver<CopyDone>) {
    let (sender, receiver) = mpsc::channel();
    let sink: CopySink = Arc::new(move |done| {
        let _ = sender.send(done);
    });
    (sink, receiver)
}

fn refused(refusal: &CopyRefusal) -> CopyResult {
    CopyResult::Refused(refusal.text())
}

/// The answer for a write that may have landed (NC168): the UI reloads the target for it.
fn unconfirmed() -> CopyResult {
    CopyResult::Unconfirmed(CopyRefusal::Unconfirmed.text())
}

fn undo_changed() -> CopyResult {
    CopyResult::Refused(navo_l10n::tr("compare-undo-changed"))
}

mod job;

#[test]
fn a_copy_writes_the_target_and_undo_puts_back_its_bytes_and_time() {
    let f = Fixture::new(NEW, OLD, Plan::Pass);
    assert_eq!(f.copy(f.changes()), CopyResult::Copied);
    assert_eq!(f.right(), NEW);
    assert_ne!(f.right_ms(), u128::from(OLD_MS));
    let undone = f.undo().expect("one copy to undo");
    assert_eq!(undone.result, CopyResult::Copied);
    assert_eq!(undone.targets, vec![location(&f.right_path())]);
    assert_eq!(f.right(), OLD);
    assert_eq!(f.right_ms(), u128::from(OLD_MS));
    assert!(
        f.undo().is_none(),
        "undo is not pushed, so a second undo has nothing"
    );
}

#[test]
fn an_edit_after_the_copy_refuses_the_undo_and_drops_it() {
    // NC169: the entry can never match again, so it leaves the stack.
    let f = Fixture::new(NEW, OLD, Plan::Pass);
    assert_eq!(f.copy(f.changes()), CopyResult::Copied);
    fs::write(f.right_path(), b"edited\n").unwrap();
    let first = f.undo().expect("the copy is on the stack");
    assert_eq!(first.result, undo_changed());
    assert_eq!(first.targets, vec![location(&f.right_path())]);
    assert_eq!(f.right(), b"edited\n");
    assert!(f.undo().is_none(), "the changed entry is dropped");
}

#[test]
fn a_pick_that_composes_the_target_writes_nothing() {
    // NC167: line 2 of the left is `c`, outside the right-only change, so the result is the
    // target itself: no job, no undo entry, and the nothing-to-copy refusal.
    let f = Fixture::new(b"a\nc\n", b"a\nb\nc\n", Plan::Pass);
    let mut changes = f.changes();
    changes[0].lines = Some(BTreeSet::from([2]));
    assert_eq!(f.copy(changes), refused(&CopyRefusal::NotCopyable));
    assert_eq!(f.right(), b"a\nb\nc\n");
    assert_eq!(f.jobs(), 0, "no job folder");
    assert!(f.undo().is_none());
}

#[test]
fn picked_lines_of_two_changes_copy_as_one_job_and_undo_as_one() {
    let f = Fixture::new(
        b"a\nb1\nb2\nc\nd\ne\nf\ng\nh1\nh2\n",
        b"a\nB1\nB2\nc\nd\ne\nf\ng\nH1\nH2\n",
        Plan::Pass,
    );
    let mut changes = f.changes();
    assert_eq!(changes.len(), 2);
    changes[0].lines = Some(BTreeSet::from([2]));
    changes[1].lines = Some(BTreeSet::from([9]));
    assert_eq!(f.copy(changes), CopyResult::Copied);
    assert_eq!(f.right(), b"a\nB1\nb2\nc\nd\ne\nf\ng\nH1\nh2\n");
    assert_eq!(f.jobs(), 1);
    assert_eq!(f.undo().unwrap().result, CopyResult::Copied);
    assert_eq!(f.right(), b"a\nB1\nB2\nc\nd\ne\nf\ng\nH1\nH2\n");
    assert!(f.undo().is_none());
}

#[test]
fn a_whole_change_mixed_with_picked_lines_is_refused() {
    let f = Fixture::new(
        b"a\nb1\nb2\nc\nd\ne\nf\ng\nh1\nh2\n",
        b"a\nB1\nB2\nc\nd\ne\nf\ng\nH1\nH2\n",
        Plan::Pass,
    );
    let mut changes = f.changes();
    changes[1].lines = Some(BTreeSet::from([9]));
    assert_eq!(f.copy(changes), refused(&CopyRefusal::NotCopyable));
    assert_eq!(f.jobs(), 0);
}

#[test]
fn a_target_changed_since_shown_is_refused_with_nothing_written() {
    let f = Fixture::new(NEW, OLD, Plan::Pass);
    let changes = f.changes();
    fs::write(f.right_path(), b"alpha\ngamma\nmore\n").unwrap();
    assert_eq!(f.copy(changes), refused(&CopyRefusal::Changed));
    assert_eq!(f.right(), b"alpha\ngamma\nmore\n");
    assert_eq!(f.jobs(), 0);
    assert!(f.undo().is_none());
}

#[test]
fn a_read_only_target_is_refused_with_nothing_written() {
    let f = Fixture::new(NEW, OLD, Plan::Pass);
    let changes = f.changes();
    let mut permissions = fs::metadata(f.right_path()).unwrap().permissions();
    permissions.set_readonly(true);
    fs::set_permissions(f.right_path(), permissions.clone()).unwrap();
    assert_eq!(f.copy(changes), refused(&CopyRefusal::ReadOnly));
    assert_eq!(f.right(), OLD);
    assert!(f.undo().is_none());
    #[expect(
        clippy::permissions_set_readonly_false,
        reason = "the test folder must be removable on Windows"
    )]
    permissions.set_readonly(false);
    fs::set_permissions(f.right_path(), permissions).unwrap();
}

/// A copy whose fill panics after the job began, as a bug in compose would.
fn panicking_copy(journal: &Journal, requests: &[CopyRequest]) -> Result<Committed, CopyRefusal> {
    let (_, target) = ends_of(&requests[0])?;
    let path = navo_fs::os_path(&target).unwrap();
    let mut job = journal.begin(JobKind::Copy, "Copy", None).unwrap();
    let mut fill = |out: &mut dyn Write| -> Result<(), String> {
        out.write_all(b"half").unwrap();
        panic!("compose died");
    };
    job.overwrite(&path, Expect::default(), None, &mut fill)
        .unwrap();
    unreachable!("the fill panics")
}

#[test]
fn a_panic_inside_the_fill_answers_refused_and_rolls_the_job_back() {
    let f = Fixture::build(NEW, OLD, Plan::Pass, Some(panicking_copy));
    let result = f.copy(f.changes());
    assert!(matches!(result, CopyResult::Refused(_)), "{result:?}");
    assert_eq!(f.right(), OLD);
    let leftovers: Vec<_> = fs::read_dir(f.work.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(leftovers.len(), 2, "no staged file is left: {leftovers:?}");
    assert!(
        f.undo().is_none(),
        "nothing pushed, and the write slot is free again"
    );
}

/// The op counts of a clean run: after open, after the copy, after the undo.
fn clean_ops() -> (u64, u64, u64) {
    let f = Fixture::new(NEW, OLD, Plan::Pass);
    let opened = f.disk.ops();
    assert_eq!(f.copy(f.changes()), CopyResult::Copied);
    let copied = f.disk.ops();
    assert_eq!(f.undo().unwrap().result, CopyResult::Copied);
    (opened, copied, f.disk.ops())
}

#[test]
fn one_fault_at_any_op_keeps_the_file_whole_and_the_copy_undoable() {
    let (opened, copied, undone) = clean_ops();
    assert!(copied > opened && undone > copied);
    let unconfirmed = unconfirmed();
    let (mut copies_in_doubt, mut undos_in_doubt) = (0, 0);
    for n in opened + 1..=undone {
        let f = Fixture::new(NEW, OLD, Plan::FailAt(n, io::ErrorKind::Other));
        let changes = f.changes();
        let result = f.copy(changes);
        let now = f.right();
        assert!(now == OLD || now == NEW, "op {n}: the file is old or new");
        if result == unconfirmed {
            // NC168: a commit in doubt keeps the new bytes and is never undoable now.
            copies_in_doubt += 1;
            assert_eq!(now, NEW, "op {n}");
        }
        if result != CopyResult::Copied {
            assert!(
                f.undo().is_none(),
                "op {n}: a refused copy is never undoable"
            );
            continue;
        }
        assert_eq!(now, NEW, "op {n}");
        let first = f.undo().expect("op {n}: a copied job is undoable");
        if first.result == unconfirmed {
            // NC169: an undo in doubt is dropped; its bytes are back already.
            undos_in_doubt += 1;
            assert!(f.undo().is_none(), "op {n}: an undo in doubt is dropped");
        } else if first.result != CopyResult::Copied {
            let now = f.right();
            assert!(now == OLD || now == NEW, "op {n}: the undo left old or new");
            // One fault is spent: a retry either undoes, or finds the old bytes back already.
            let retry = f.undo().expect("op {n}: a failed undo stays on the stack");
            assert!(
                retry.result == CopyResult::Copied || now == OLD,
                "op {n}: {retry:?}"
            );
        }
        assert_eq!(f.right(), OLD, "op {n}: the copy is undone");
    }
    assert!(
        copies_in_doubt > 0,
        "a fault at the commit sync is in doubt"
    );
    assert!(
        undos_in_doubt > 0,
        "a fault at the undo's commit sync is in doubt"
    );
}

#[test]
fn a_lasting_fault_never_leaves_a_refused_copy_undoable() {
    let (opened, copied, _) = clean_ops();
    let mut left_new = 0;
    for n in opened + 1..=copied {
        let f = Fixture::new(NEW, OLD, Plan::FailFrom(n, io::ErrorKind::Other));
        let result = f.copy(f.changes());
        let now = f.right();
        assert!(now == OLD || now == NEW, "op {n}: the file is old or new");
        assert_ne!(result, CopyResult::Copied, "op {n}: the copy cannot finish");
        // NC168: new bytes left by a commit in doubt or a rollback left to the next start
        // are never answered as "not copied".
        if now == NEW {
            left_new += 1;
            assert_eq!(result, unconfirmed(), "op {n}");
        }
        assert!(
            f.undo().is_none(),
            "op {n}: a refused copy is never undoable"
        );
    }
    assert!(left_new > 0, "a lasting fault can leave the new bytes");
}

#[test]
fn a_write_in_flight_refuses_a_second_one_as_running() {
    let f = Fixture::new(NEW, OLD, Plan::Pass);
    let host = f.host.clone();
    let busy = host.try_busy().expect("the write slot is free");
    let (sink, receiver) = copy_sink();
    f.backend.copy_changes(f.changes(), sink);
    let running = CopyResult::Refused(navo_l10n::tr("compare-copy-running"));
    assert_eq!(receiver.recv_timeout(WAIT).unwrap().result, running);
    assert_eq!(f.undo().unwrap().result, running);
    drop(busy);
    assert_eq!(f.copy(f.changes()), CopyResult::Copied);
    assert_eq!(f.right(), NEW);
}

/// A copy that swaps the target between its two read steps, as an editor's save by rename.
fn swapping_copy(journal: &Journal, requests: &[CopyRequest]) -> Result<Committed, CopyRefusal> {
    let (_, target) = ends_of(&requests[0])?;
    let path = navo_fs::os_path(&target).unwrap();
    let mut swap = || {
        let staged = path.with_extension("saving");
        fs::write(&staged, SAVED).unwrap();
        fs::rename(&staged, &path).unwrap();
    };
    copy_between(journal, requests, &mut swap)
}

/// What the editor saves: the shown target with a same-size fix outside the change.
const SAVED: &[u8] = b"a\nb\nc\nd\ne\nf\ng\nX\n";

#[test]
fn a_rename_over_the_target_during_the_copy_is_refused_as_changed() {
    let f = Fixture::build(
        b"a\nB\nc\nd\ne\nf\ng\nh\n",
        b"a\nb\nc\nd\ne\nf\ng\nh\n",
        Plan::Pass,
        Some(swapping_copy),
    );
    assert_eq!(f.copy(f.changes()), refused(&CopyRefusal::Changed));
    assert_eq!(f.right(), SAVED, "the editor's save is kept");
    assert!(f.undo().is_none());
}

#[test]
fn an_empty_change_list_answers_not_copyable() {
    let (sink, receiver) = copy_sink();
    let f = Fixture::new(NEW, OLD, Plan::Pass);
    f.host.copy_changes(Vec::new(), sink);
    let end = receiver.recv_timeout(WAIT).expect("an answer");
    assert_eq!(end.result, refused(&CopyRefusal::NotCopyable));
    assert_eq!(f.jobs(), 0);
}

#[test]
fn a_panic_keeps_the_write_slot_until_the_refusal_is_built() {
    let f = Fixture::new(NEW, OLD, Plan::Pass);
    let host = f.host.clone();
    let busy = host.try_busy().expect("the write slot is free");
    let (sender, receiver) = mpsc::channel();
    let sink: Arc<dyn Fn(bool) + Send + Sync> = Arc::new(move |held| {
        let _ = sender.send(held);
    });
    let probe = host.clone();
    let worker = std::thread::spawn(move || {
        let refused = move || probe.try_busy().is_none();
        super::guarded(sink, refused, busy, || -> bool { panic!("undo died") });
    });
    assert!(worker.join().is_err(), "the work panicked");
    assert!(
        receiver.recv_timeout(WAIT).unwrap(),
        "held while the refusal ran"
    );
    assert!(host.try_busy().is_some(), "freed after the panic");
}
