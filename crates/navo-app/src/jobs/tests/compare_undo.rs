// Tests for offering and undoing compare Save jobs through files.undo.
// They cover one-file and many-file saves, and the text shown for retained versions.

use std::sync::Arc;

use navo_core::retention::RetentionId;
use navo_core::{FileStamp, Location, OperationPlan, WriteTarget, write_many_plan};
use navo_fs::ProviderOps;
use navo_store::{JobId, JobState, StoreDirs};
use navo_ui::browser::{LoggedOperation, RenameReport, UndoKind, UndoOffer};

use super::super::{SessionHolds, lock};
use super::*;

const OLD: u64 = 3;
const NEW: u64 = 11;
const OLD_TIME: i64 = 1_000;
const NEW_TIME: i64 = 2_000;
const OLD_HASH: [u8; 32] = [1; 32];
const NEW_HASH: [u8; 32] = [2; 32];
const OFF: &str = "";

fn seen() -> FileStamp {
    FileStamp {
        size: OLD,
        modified_ms: Some(OLD_TIME),
        hash: OLD_HASH,
    }
}

struct Session {
    _root: tempfile::TempDir,
    dirs: StoreDirs,
    journal: SharedJournal,
    holds: SessionHolds,
    fs: Arc<FakeFs>,
}

impl Session {
    fn new(settings: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let dirs = StoreDirs::in_profile(root.path());
        dirs.ensure().unwrap();
        std::fs::write(dirs.settings_file(), settings).unwrap();
        let journal = open(&dirs.journal_file()).unwrap();
        let holds = SessionHolds::new(Arc::clone(&journal), dirs.clone());
        let fs = Arc::new(FakeFs::new());
        fs.add_folder("dest");
        fs.add_folder("stage");
        Self {
            _root: root,
            dirs,
            journal,
            holds,
            fs,
        }
    }

    fn put(&self, path: &str, (size, modified): (u64, i64), hash: [u8; 32]) {
        let (folder, name) = path.rsplit_once('/').unwrap();
        self.fs.add_entry(
            folder,
            FsEntry {
                name: ProviderPath::from_text(name),
                kind: EntryKind::File,
                size,
                modified_ms: Some(modified),
                attributes: Attributes::default(),
            },
        );
        self.fs.set_hash(path, hash);
    }

    fn plan(&self, count: usize) -> OperationPlan {
        let mut writes = Vec::new();
        for index in 0..count {
            let target = format!("dest/{index}.txt");
            let staged = format!("stage/{index}.part");
            self.put(&target, (OLD, OLD_TIME), OLD_HASH);
            self.put(&staged, (NEW, NEW_TIME), NEW_HASH);
            writes.push(WriteTarget {
                target: self.fs.location(&target),
                staged: self.fs.location(&staged),
                expect: seen(),
                staged_len: NEW,
            });
        }
        write_many_plan(&writes).unwrap()
    }

    fn save(&self, plan: &OperationPlan) {
        self.save_with(&self.holds.compare_factory(None), plan);
    }

    fn save_with(&self, factory: &navo_ui::browser::LogFactory, plan: &OperationPlan) {
        let mut log = factory(plan).unwrap();
        log.check(LoggedOperation {
            plan,
            ops: self.fs.as_ref(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
            waiting: &mut |_| {},
            retain_deletes: None,
        })
        .unwrap();
        let outcome = run_job(
            plan,
            &[],
            &mut JobContext {
                ops: self.fs.as_ref(),
                log: log.as_mut(),
                cancel: &CancelToken::new(),
                progress: &mut |_| {},
            },
        );
        assert_eq!(outcome, JobOutcome::Done);
        log.finish(&outcome, self.fs.as_ref()).unwrap();
    }

    fn offer(&self) -> UndoOffer {
        let hooks = crate::rename_hooks::hooks(Some(Arc::clone(&self.journal)), None);
        (hooks.last)().unwrap().expect("a save is offered")
    }

    fn undo(&self, offer: &UndoOffer) {
        let (outcome, report) = self.undo_result(offer);
        assert_eq!(outcome, JobOutcome::Done);
        assert_eq!(report.missing, 0);
    }

    fn undo_result(&self, offer: &UndoOffer) -> (JobOutcome, RenameReport) {
        let job = JobId(i64::try_from(offer.job).unwrap());
        let mut log = crate::jobs::undo_log(&self.journal, job).unwrap();
        let outcome = navo_ui::browser::run_undo(
            &offer.plan,
            &offer.done,
            &mut JobContext {
                ops: self.fs.as_ref(),
                log: log.as_mut(),
                cancel: &CancelToken::new(),
                progress: &mut |_| {},
            },
        );
        log.finish(&outcome.0, self.fs.as_ref()).unwrap();
        outcome
    }

    fn restore_retained(&self, offer: &UndoOffer) -> Vec<Location> {
        let UndoKind::Retained(ids) = &offer.kind else {
            panic!("a retained offer");
        };
        self.restore_ids(ids)
    }

    fn ops(&self) -> Arc<dyn ProviderOps> {
        self.fs.clone()
    }

    fn save_retained(&self, plan: &OperationPlan) {
        self.save_with(&self.holds.compare_factory(Some(self.ops())), plan);
    }

    fn restore_ids(&self, ids: &[RetentionId]) -> Vec<Location> {
        let hooks = crate::retained_hooks::hooks(Some(&self.dirs), true, self.ops());
        ids.iter()
            .map(|id| hooks.restore(*id).expect("restores"))
            .collect()
    }

    fn hash(&self, path: &str) -> [u8; 32] {
        self.fs
            .content_hash(&self.fs.location(path), &CancelToken::new())
            .unwrap()
    }

    fn state(&self, offer: &UndoOffer) -> JobState {
        let job = JobId(i64::try_from(offer.job).unwrap());
        lock(&self.journal).read_job(job).unwrap().0
    }
}

#[test]
fn a_one_step_save_is_offered_and_restores_in_place() {
    let session = Session::new(OFF);
    session.save(&session.plan(1));
    let offer = session.offer();
    assert_eq!(offer.plan.kind, OpKind::Save);
    assert_eq!(offer.items(), 1);
    session.undo(&offer);
    assert_eq!(session.hash("dest/0.txt"), OLD_HASH);
    assert_eq!(session.state(&offer), JobState::Undone);
}

#[test]
fn a_three_step_save_is_offered_with_all_its_files() {
    let session = Session::new(OFF);
    session.save(&session.plan(3));
    let offer = session.offer();
    assert_eq!(offer.plan.kind, OpKind::Save);
    assert_eq!(offer.items(), 3);
    assert_eq!(offer.done, [0, 1, 2]);
    session.undo(&offer);
    for index in 0..3 {
        assert_eq!(session.hash(&format!("dest/{index}.txt")), OLD_HASH);
    }
    assert_eq!(session.state(&offer), JobState::Undone);
}

#[test]
fn the_retained_text_says_where_the_old_version_lands() {
    let text = navo_l10n::t!("undo-retained-ask-summary");
    assert!(text.contains("beside"), "{text}");
    assert!(text.contains("(restored)"), "{text}");
}

#[test]
fn the_save_text_names_the_file_count() {
    let text = navo_l10n::t!("undo-save-ask", count = 3_u64);
    assert!(text.contains('3'), "{text}");
    assert!(text.contains("files"), "{text}");
}

#[test]
fn retention_on_restores_an_unchanged_target_in_place() {
    let session = Session::new("[retention]\nretain-overwrites = true\n");
    session.save_retained(&session.plan(1));
    let offer = session.offer();
    assert!(matches!(offer.kind, UndoKind::Retained(_)));
    let places = session.restore_retained(&offer);
    assert_eq!(places, [session.fs.location("dest/0.txt")]);
    assert_eq!(session.hash("dest/0.txt"), OLD_HASH);
    assert!(
        session
            .fs
            .metadata(&session.fs.location("dest/0 (restored).txt"))
            .is_err()
    );
}

#[test]
fn retention_on_keeps_a_restored_copy_beside_an_edited_target() {
    let session = Session::new("[retention]\nretain-overwrites = true\n");
    session.save_retained(&session.plan(1));
    let offer = session.offer();
    session.put("dest/0.txt", (NEW + 1, 5_000), [9; 32]);
    let places = session.restore_retained(&offer);
    assert_eq!(places, [session.fs.location("dest/0 (restored).txt")]);
    assert_eq!(session.hash("dest/0.txt"), [9; 32]);
    assert_eq!(session.hash("dest/0 (restored).txt"), OLD_HASH);
}

#[test]
fn a_done_step_without_a_held_row_fails_visibly() {
    let session = Session::new(OFF);
    session.save(&session.plan(1));
    let offer = session.offer();
    let job = JobId(i64::try_from(offer.job).unwrap());
    lock(&session.journal).forget_held(job).unwrap();
    let (outcome, report) = session.undo_result(&offer);
    assert!(matches!(outcome, JobOutcome::Failed { .. }), "{outcome:?}");
    assert_eq!(report.missing, 1);
}
