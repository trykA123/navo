// Tests for restoring retained compare saves: the newer bytes and undo rows are freed,
// and a restore whose catalog mark failed refuses the retry with a clear reason (VR2 lane A).

use navo_core::FsErrorKind;
use navo_fs::{FakeOp, Fault, HeldFile};

use super::*;

const RETAIN: &str = "[retention]\nretain-overwrites = true\n";

impl Session {
    /// The retained ids and the job of the last save.
    fn retained(&self) -> (Vec<RetentionId>, JobId) {
        let offer = self.offer();
        let UndoKind::Retained(ids) = offer.kind else {
            panic!("a retained offer");
        };
        (ids, JobId(i64::try_from(offer.job).unwrap()))
    }

    /// The held old file of `target` that in-place undo reads.
    fn held_of(&self, job: JobId, target: &str) -> Option<HeldFile> {
        crate::jobs::save_held(&self.journal, job, &self.fs.location(target)).unwrap()
    }

    fn rows(&self, job: JobId) -> usize {
        lock(&self.journal).held(job).unwrap().len()
    }

    fn try_restore(&self, id: RetentionId) -> Result<Location, String> {
        crate::retained_hooks::hooks(Some(&self.dirs), true, self.ops()).restore(id)
    }

    fn missing(&self, at: &Location) -> bool {
        self.fs
            .metadata(at)
            .is_err_and(|error| error.kind == FsErrorKind::NotFound)
    }
}

/// The `.current` file in-place undo parks the newer bytes in.
fn current_of(file: &HeldFile) -> Location {
    let name = file.held.name().unwrap().to_text();
    file.held
        .parent()
        .unwrap()
        .join(&ProviderPath::from_text(&format!("{name}.current")))
        .unwrap()
}

/// Two retained one-file saves: `dest/0.txt` first, then `dest/1.txt`.
fn two_saves(session: &Session) -> [(RetentionId, JobId); 2] {
    session.save_retained(&session.plan(1));
    let (first, first_job) = session.retained();
    session.put("dest/1.txt", (OLD, OLD_TIME), OLD_HASH);
    session.put("stage/1.part", (NEW, NEW_TIME), NEW_HASH);
    let at = |path| session.fs.location(path);
    session.save_retained(&navo_core::write_plan(
        &at("dest/1.txt"),
        &at("stage/1.part"),
        seen(),
        NEW,
    ));
    let (later, later_job) = session.retained();
    [(first[0], first_job), (later[0], later_job)]
}

#[test]
fn an_in_place_restore_frees_the_newer_bytes_and_a_second_entry_still_restores() {
    let session = Session::new(RETAIN);
    let [(first_id, first_job), (later_id, later_job)] = two_saves(&session);
    let first = session.held_of(first_job, "dest/0.txt").unwrap();
    let later = session.held_of(later_job, "dest/1.txt").unwrap();
    assert_eq!(
        session.restore_ids(&[first_id]),
        [session.fs.location("dest/0.txt")]
    );
    assert_eq!(session.hash("dest/0.txt"), OLD_HASH);
    assert!(session.missing(&current_of(&first)), "no .current is left");
    assert_eq!(
        session.restore_ids(&[later_id]),
        [session.fs.location("dest/1.txt")]
    );
    assert_eq!(session.hash("dest/1.txt"), OLD_HASH);
    assert!(session.missing(&current_of(&later)), "no .current is left");
}

#[test]
fn an_in_place_restore_forgets_its_undo_rows_and_keeps_the_others() {
    let session = Session::new(RETAIN);
    let [(first_id, first_job), (_, later_job)] = two_saves(&session);
    let later_rows = session.rows(later_job);
    assert!(session.rows(first_job) > 0);
    session.restore_ids(&[first_id]);
    assert_eq!(session.rows(first_job), 0);
    assert_eq!(session.rows(later_job), later_rows);
    assert!(session.held_of(later_job, "dest/1.txt").is_some());
}

#[test]
fn a_restore_beside_an_edited_target_forgets_its_undo_row() {
    let session = Session::new(RETAIN);
    session.save_retained(&session.plan(1));
    let (ids, job) = session.retained();
    session.put("dest/0.txt", (NEW + 1, 5_000), [9; 32]);
    assert_eq!(
        session.restore_ids(&ids),
        [session.fs.location("dest/0 (restored).txt")]
    );
    assert_eq!(session.rows(job), 0);
}

#[test]
fn one_failed_catalog_mark_is_retried_and_the_restore_completes() {
    let session = Session::new(RETAIN);
    session.save_retained(&session.plan(1));
    let (ids, job) = session.retained();
    let first = session.fs.calls("dest", FakeOp::Identity) + 1;
    let fault = Fault::Fail(FsErrorKind::Locked);
    session.fs.fault("dest", FakeOp::Identity, first, fault);
    assert_eq!(
        session.restore_ids(&ids),
        [session.fs.location("dest/0.txt")]
    );
    assert_eq!(session.hash("dest/0.txt"), OLD_HASH);
    assert_eq!(session.rows(job), 0);
}

#[test]
fn a_restore_whose_mark_failed_twice_refuses_the_retry_with_a_clear_reason() {
    let session = Session::new(RETAIN);
    session.save_retained(&session.plan(1));
    let (ids, _) = session.retained();
    let first = session.fs.calls("dest", FakeOp::Identity) + 1;
    for call in [first, first + 1] {
        let fault = Fault::Fail(FsErrorKind::Locked);
        session.fs.fault("dest", FakeOp::Identity, call, fault);
    }
    assert!(session.try_restore(ids[0]).is_err());
    assert_eq!(
        session.hash("dest/0.txt"),
        OLD_HASH,
        "the old bytes are back"
    );
    let refusal = session.try_restore(ids[0]).unwrap_err();
    assert!(
        refusal.contains("no longer in the holding folder"),
        "{refusal}"
    );
    assert_eq!(session.hash("dest/0.txt"), OLD_HASH);
    assert!(session.missing(&session.fs.location("dest/0 (restored).txt")));
}
