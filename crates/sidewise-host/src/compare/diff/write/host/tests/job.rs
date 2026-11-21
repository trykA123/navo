//! Tests of the undo stack (NC148, NC166): newest first, a dropped entry, and the toast's
//! Undo by job id: an older copy is `NotNewest`, an undone one `Gone`, and neither writes.

use super::*;

impl Fixture {
    /// Copy `changes` as one job and return the whole end, with its job id.
    pub(super) fn copy_done(&self, changes: Vec<CopyChange>) -> CopyDone {
        let (sink, receiver) = copy_sink();
        self.backend.copy_changes(changes, sink);
        let end = receiver.recv_timeout(WAIT).unwrap();
        assert_eq!((end.request, end.direction), (9, CopyDirection::ToRight));
        assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());
        end
    }

    /// Undo copy `job` through the backend; the end, when it started.
    fn undo_job(&self, job: &str) -> (UndoStart, Option<UndoDone>) {
        let (sender, receiver) = mpsc::channel();
        let sink: UndoSink = Arc::new(move |done| {
            let _ = sender.send(done);
        });
        let start = self.backend.undo_last(Some(job), sink);
        if start != UndoStart::Started {
            assert!(receiver.recv_timeout(Duration::from_millis(50)).is_err());
            return (start, None);
        }
        (start, Some(receiver.recv_timeout(WAIT).unwrap()))
    }
}

#[test]
fn only_the_newest_copy_undoes_by_its_job_id() {
    let f = Fixture::new(
        b"a\nB\nc\nd\ne\nf\ng\nH\n",
        b"a\nb\nc\nd\ne\nf\ng\nh\n",
        Plan::Pass,
    );
    let mut changes = f.changes();
    changes.pop();
    let first = f.copy_done(changes).job.expect("first job id");
    let mut again = f.changes();
    let second = f
        .copy_done(vec![again.remove(0)])
        .job
        .expect("second job id");
    assert_ne!(first, second);
    assert_eq!(f.undo_job(&first), (UndoStart::NotNewest, None));
    assert_eq!(f.right(), b"a\nB\nc\nd\ne\nf\ng\nH\n", "nothing undone");
    let (start, done) = f.undo_job(&second);
    assert_eq!(start, UndoStart::Started);
    assert_eq!(done.unwrap().result, CopyResult::Copied);
    assert_eq!(f.right(), b"a\nB\nc\nd\ne\nf\ng\nh\n");
    assert_eq!(f.undo_job(&first).0, UndoStart::Started);
    assert_eq!(f.right(), b"a\nb\nc\nd\ne\nf\ng\nh\n");
    assert_eq!(
        f.undo_job(&second),
        (UndoStart::Gone, None),
        "already undone"
    );
}

/// Copy 1, copy 2, Ctrl+Z (undoes 2), then copy 2's toast Undo: `Gone`, and copy 1 stays.
#[test]
fn an_undone_copy_is_gone_and_leaves_the_older_copy_on_the_stack() {
    let f = Fixture::new(
        b"a\nB\nc\nd\ne\nf\ng\nH\n",
        b"a\nb\nc\nd\ne\nf\ng\nh\n",
        Plan::Pass,
    );
    let mut changes = f.changes();
    changes.pop();
    let first = f.copy_done(changes).job.expect("first job id");
    let second = f.copy_done(f.changes()).job.expect("second job id");
    assert_eq!(f.undo().unwrap().result, CopyResult::Copied);
    assert_eq!(f.undo_job(&second), (UndoStart::Gone, None));
    assert_eq!(f.right(), b"a\nB\nc\nd\ne\nf\ng\nh\n", "nothing undone");
    assert_eq!(f.undo_job(&first).0, UndoStart::Started);
    assert_eq!(f.right(), b"a\nb\nc\nd\ne\nf\ng\nh\n");
}

#[test]
fn undo_goes_back_one_copy_at_a_time_newest_first() {
    let f = Fixture::new(
        b"a\nB\nc\nd\ne\nf\ng\nH\n",
        b"a\nb\nc\nd\ne\nf\ng\nh\n",
        Plan::Pass,
    );
    let mut changes = f.changes();
    assert_eq!(changes.len(), 2);
    let second = changes.pop().unwrap();
    assert_eq!(f.copy(changes), CopyResult::Copied);
    let mut again = f.changes();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].right_bytes, second.right_bytes);
    assert_eq!(f.copy(vec![again.remove(0)]), CopyResult::Copied);
    assert_eq!(f.right(), b"a\nB\nc\nd\ne\nf\ng\nH\n");
    assert_eq!(f.undo().unwrap().result, CopyResult::Copied);
    assert_eq!(f.right(), b"a\nB\nc\nd\ne\nf\ng\nh\n");
    assert_eq!(f.undo().unwrap().result, CopyResult::Copied);
    assert_eq!(f.right(), b"a\nb\nc\nd\ne\nf\ng\nh\n");
    assert!(f.undo().is_none());
}

#[test]
fn a_dropped_changed_undo_leaves_the_older_copy_undoable() {
    let f = Fixture::new(
        b"a\nB\nc\nd\ne\nf\ng\nH\n",
        b"a\nb\nc\nd\ne\nf\ng\nh\n",
        Plan::Pass,
    );
    let mut changes = f.changes();
    let _second = changes.pop().unwrap();
    assert_eq!(f.copy(changes), CopyResult::Copied);
    let (after_first, first_time) = (f.right(), fs::metadata(f.right_path()).unwrap());
    assert_eq!(f.copy(f.changes()), CopyResult::Copied);
    // An edit that puts back exactly what the first copy wrote, time included.
    fs::write(f.right_path(), &after_first).unwrap();
    let file = fs::File::options()
        .write(true)
        .open(f.right_path())
        .unwrap();
    file.set_modified(first_time.modified().unwrap()).unwrap();
    drop(file);
    assert_eq!(f.undo().unwrap().result, undo_changed());
    assert_eq!(
        f.undo().unwrap().result,
        CopyResult::Copied,
        "the older entry re-checks"
    );
    assert_eq!(f.right(), b"a\nb\nc\nd\ne\nf\ng\nh\n");
    assert!(f.undo().is_none());
}

#[test]
fn a_backend_without_a_host_has_nothing_to_undo() {
    let sink: UndoSink = Arc::new(|_| panic!("no undo to report"));
    assert_eq!(
        DiffBackend::default().undo_last(None, sink),
        UndoStart::Nothing
    );
}
