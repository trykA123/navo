use std::ops::ControlFlow;
use std::sync::Arc;

use navo_core::retention::RetentionEntry;
use navo_core::{FileStamp, OperationPlan, write_plan};
use navo_fs::{
    CancelToken, EntryBatch, FakeFs, FakeOp, HELD_PREFIX, JobContext, JobOutcome, Provider,
    ProviderOps, run_job,
};
use navo_store::{Db, JobState, RetentionCatalog, StoreDirs};
use navo_ui::browser::{LoggedOperation, OpLog};

use super::super::{SharedJournal, experimental_factory, lock, open, recover_profile};
use super::dying::{Dying, Halting};

pub(super) const TARGET: &str = "dest/a.txt";
pub(super) const STAGED: &str = "dest/.a.txt.part";
pub(super) const OLD_HASH: [u8; 32] = [1; 32];
pub(super) const NEW_HASH: [u8; 32] = [2; 32];

pub(super) struct Saving {
    pub _root: tempfile::TempDir,
    pub dirs: StoreDirs,
    pub journal: SharedJournal,
    pub fs: Arc<FakeFs>,
}

pub(super) fn put(fs: &FakeFs, path: &str, (size, modified): (u64, i64), hash: [u8; 32]) {
    let (folder, name) = path.rsplit_once('/').unwrap_or(("", path));
    fs.add_entry(
        folder,
        navo_fs::FsEntry {
            name: navo_core::ProviderPath::from_text(name),
            kind: navo_fs::EntryKind::File,
            size,
            modified_ms: Some(modified),
            attributes: navo_fs::Attributes::default(),
        },
    );
    fs.set_hash(path, hash);
}

pub(super) fn hash_at(fs: &FakeFs, at: &navo_core::Location) -> Option<[u8; 32]> {
    fs.content_hash(at, &CancelToken::new()).ok()
}

impl Saving {
    pub(super) fn new(settings: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let dirs = StoreDirs::in_profile(root.path());
        dirs.ensure().unwrap();
        if !settings.is_empty() {
            std::fs::write(dirs.settings_file(), settings).unwrap();
        }
        let journal = open(&dirs.journal_file()).unwrap();
        let fs = Arc::new(FakeFs::new());
        fs.add_folder("dest");
        fs.set_available_space("dest", Some(u64::MAX));
        put(&fs, TARGET, (3, 1_000), OLD_HASH);
        put(&fs, STAGED, (11, 2_000), NEW_HASH);
        Self {
            _root: root,
            dirs,
            journal,
            fs,
        }
    }

    pub(super) fn plan(&self, staged: &str, (size, modified): (u64, i64)) -> OperationPlan {
        let staged_len = self.fs.metadata(&self.fs.location(staged)).unwrap().size;
        let seen = FileStamp {
            size,
            modified_ms: Some(modified),
            hash: hash_at(&self.fs, &self.fs.location(TARGET)).unwrap(),
        };
        write_plan(
            &self.fs.location(TARGET),
            &self.fs.location(staged),
            seen,
            staged_len,
        )
    }

    pub(super) fn start(&self, plan: &OperationPlan) -> Box<dyn OpLog> {
        let mut log = experimental_factory(self.journal.clone(), self.dirs.clone())(plan).unwrap();
        log.check(LoggedOperation {
            plan,
            ops: self.fs.as_ref(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
            waiting: &mut |_| {},
            retain_deletes: None,
        })
        .unwrap();
        log
    }

    pub(super) fn run(
        &self,
        plan: &OperationPlan,
        ops: &Dying<'_>,
        marked: bool,
    ) -> (Halting, JobOutcome) {
        let mut log = Halting {
            inner: self.start(plan),
            dead: ops.dead.clone(),
            dies_when_marked: marked,
        };
        let outcome = run_job(
            plan,
            &[],
            &mut JobContext {
                ops,
                log: &mut log,
                cancel: &CancelToken::new(),
                progress: &mut |_| {},
            },
        );
        (log, outcome)
    }

    pub(super) fn save(&self, staged: &str, seen: (u64, i64)) -> JobOutcome {
        let plan = self.plan(staged, seen);
        let ops = Dying::never(&self.fs);
        let (mut log, outcome) = self.run(&plan, &ops, false);
        log.inner.finish(&outcome, self.fs.as_ref()).unwrap();
        outcome
    }

    pub(super) fn entries(&self) -> Vec<RetentionEntry> {
        let (db, _) = Db::open(&self.dirs.journal_file(), false).unwrap();
        RetentionCatalog::new(db)
            .unwrap()
            .list_after(0, 100)
            .unwrap()
    }

    pub(super) fn hash(&self, path: &str) -> Option<[u8; 32]> {
        hash_at(&self.fs, &self.fs.location(path))
    }

    fn holders(&self) -> Vec<String> {
        holders(&self.fs)
    }

    fn recovered(&self) -> JobState {
        let report = recover_profile(
            &self.journal,
            self.fs.as_ref(),
            (&self.dirs.data, &[]),
            true,
        )
        .unwrap();
        assert!(report.pending.is_empty());
        assert!(lock(&self.journal).restore_pending().unwrap().is_empty());
        lock(&self.journal)
            .read_job(navo_store::JobId(1))
            .unwrap()
            .0
    }
}

fn holder_at(op: FakeOp) -> (String, FakeOp) {
    (format!("dest/{HELD_PREFIX}*"), op)
}

fn crashed(at: (&str, FakeOp), marked: bool) -> Saving {
    let saving = Saving::new("");
    let plan = saving.plan(STAGED, (3, 1_000));
    let ops = Dying::after(&saving.fs, at.0, at.1);
    drop(saving.run(&plan, &ops, marked));
    assert!(ops.dead.load(std::sync::atomic::Ordering::SeqCst));
    saving
}

fn returned(saving: &Saving) {
    assert_eq!(saving.recovered(), JobState::Failed);
    assert_eq!(saving.hash(TARGET), Some(OLD_HASH));
    assert_eq!(saving.hash(STAGED), Some(NEW_HASH));
    assert!(saving.entries().is_empty());
}

fn retained(saving: &Saving) {
    assert_eq!(saving.recovered(), JobState::Failed);
    assert_eq!(saving.hash(TARGET), Some(NEW_HASH));
    let [entry] = saving.entries().try_into().unwrap();
    let held = entry.record.held().unwrap();
    assert_eq!(hash_at(&saving.fs, &held), Some(OLD_HASH));
    assert_eq!(saving.fs.metadata(&held).unwrap().size, 3);
}

#[test]
fn a_crash_after_making_the_holder_returns_the_save() {
    let (path, op) = holder_at(FakeOp::Create);
    returned(&crashed((&path, op), false));
}

#[test]
fn a_crash_after_hiding_the_holder_returns_the_save_and_removes_the_holder() {
    let (path, op) = holder_at(FakeOp::Secure);
    let saving = crashed((&path, op), false);
    assert_eq!(saving.holders().len(), 1);
    returned(&saving);
    assert!(saving.holders().is_empty());
}

#[test]
fn a_crash_after_holding_the_old_file_moves_it_back_and_returns_the_save() {
    let saving = crashed((TARGET, FakeOp::Rename), false);
    assert_eq!(saving.hash(TARGET), None);
    returned(&saving);
    assert!(saving.holders().is_empty());
}

#[test]
fn a_crash_after_placing_before_marking_retains_the_old_version() {
    let saving = crashed((STAGED, FakeOp::Rename), false);
    retained(&saving);
}

#[test]
fn a_crash_after_marking_the_step_retains_the_old_version() {
    let saving = crashed(("\0", FakeOp::Create), true);
    retained(&saving);
}

fn retouched(fs: &FakeFs, path: &str) {
    let modified = fs
        .metadata(&fs.location(path))
        .unwrap()
        .modified_ms
        .unwrap();
    let hash = hash_at(fs, &fs.location(path)).unwrap();
    let size = fs.metadata(&fs.location(path)).unwrap().size;
    put(fs, path, (size, modified + 5), hash);
}

fn settles_failed(saving: &Saving, log: &mut Halting, outcome: &JobOutcome) {
    log.inner.finish(outcome, saving.fs.as_ref()).unwrap();
    assert_eq!(
        lock(&saving.journal)
            .read_job(navo_store::JobId(1))
            .unwrap()
            .0,
        JobState::Failed
    );
    assert!(lock(&saving.journal).restore_pending().unwrap().is_empty());
    assert!(saving.entries().is_empty());
    assert!(saving.holders().is_empty());
}

#[test]
fn a_save_refused_for_an_old_file_touched_during_the_hold_settles_as_failed() {
    let saving = Saving::new("");
    let plan = saving.plan(STAGED, (3, 1_000));
    let ops = Dying::meddling(&saving.fs, (TARGET, FakeOp::Rename), |touched| {
        let holder = holders(touched).remove(0);
        retouched(touched, &format!("dest/{holder}/a.txt"));
    });
    let (mut log, outcome) = saving.run(&plan, &ops, false);
    assert!(matches!(outcome, JobOutcome::Failed { .. }), "{outcome:?}");
    assert_eq!(saving.hash(TARGET), Some(OLD_HASH));
    assert_eq!(saving.hash(STAGED), Some(NEW_HASH));
    settles_failed(&saving, &mut log, &outcome);
}

fn holders(fs: &FakeFs) -> Vec<String> {
    let mut names = Vec::new();
    let mut sink = |batch: EntryBatch| {
        names.extend(batch.entries.into_iter().map(|entry| entry.name.to_text()));
        ControlFlow::Continue(())
    };
    fs.list(&fs.location("dest"), &mut sink, &CancelToken::new())
        .unwrap();
    names.retain(|name| name.starts_with(HELD_PREFIX));
    names
}

#[test]
fn a_retained_copy_refused_for_a_target_touched_before_the_hold_settles_as_failed() {
    let saving = Saving::new("");
    saving.fs.add_file("source/a.txt", 7);
    let ops = saving.fs.as_ref() as &dyn ProviderOps;
    let items = navo_fs::scan(
        ops,
        &[saving.fs.location("source/a.txt")],
        &CancelToken::new(),
    )
    .unwrap();
    let target = saving.fs.location("dest");
    let existing = navo_fs::existing_at(ops, &items, &target, &CancelToken::new()).unwrap();
    let plan = navo_core::plan(&navo_core::PlanRequest {
        kind: navo_core::OpKind::Copy,
        items,
        target: Some(target),
        existing,
        policy: navo_core::ConflictPolicy::OverwriteAll,
        same_volume: true,
        protected: vec![],
    });
    let (holder, op) = holder_at(FakeOp::Secure);
    let dying = Dying::meddling(&saving.fs, (&holder, op), |touched| {
        retouched(touched, TARGET)
    });
    let (mut log, outcome) = saving.run(&plan, &dying, false);
    assert!(matches!(outcome, JobOutcome::Failed { .. }), "{outcome:?}");
    assert_eq!(saving.hash(TARGET), Some(OLD_HASH));
    settles_failed(&saving, &mut log, &outcome);
}
