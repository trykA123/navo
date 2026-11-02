use navo_core::{FileStamp, FsErrorKind, Location, write_plan};
use navo_fs::{FakeOp, Fault, ProviderOps};
use navo_store::{JobId, JobState, StoreDirs};
use navo_ui::browser::{LogFactory, LoggedOperation};

use super::super::held::held_of;
use super::super::{PreviousVersion, RestoreError, SessionHolds, profile_factory};
use super::*;

mod copies;
mod races;
mod restore_faults;

const TARGET: &str = "dest/a.txt";
const STAGED: &str = "dest/.a.txt.part";
const OLD_HASH: [u8; 32] = [1; 32];
const NEW_HASH: [u8; 32] = [2; 32];
const SAVE: JobId = JobId(1);

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
        Self {
            _root: root,
            dirs,
            journal,
            holds,
            fs: Arc::new(FakeFs::new()),
        }
    }

    fn ops(&self) -> Arc<dyn ProviderOps> {
        self.fs.clone()
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

    fn save_with(&self, factory: &LogFactory) {
        self.fs.add_folder("dest");
        self.put(TARGET, (3, 1_000), OLD_HASH);
        self.put(STAGED, (11, 2_000), NEW_HASH);
        let seen = FileStamp {
            size: 3,
            modified_ms: Some(1_000),
            hash: OLD_HASH,
        };
        let plan = write_plan(&self.at(TARGET), &self.at(STAGED), seen, 11);
        let mut log = factory(&plan).unwrap();
        log.check(LoggedOperation {
            plan: &plan,
            ops: self.fs.as_ref(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
            waiting: &mut |_| {},
            retain_deletes: None,
        })
        .unwrap();
        let outcome = run_job(
            &plan,
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

    fn save(&self) {
        self.save_with(&self.holds.factory(None));
    }

    fn at(&self, path: &str) -> Location {
        self.fs.location(path)
    }

    fn file(&self, path: &str) -> Option<(u64, Option<i64>, [u8; 32])> {
        let entry = self.fs.metadata(&self.at(path)).ok()?;
        let hash = self.fs.content_hash(&self.at(path), &CancelToken::new());
        Some((entry.size, entry.modified_ms, hash.unwrap()))
    }

    fn held(&self) -> Vec<Location> {
        held_of(&self.journal, SAVE)
            .unwrap()
            .files
            .into_iter()
            .map(|file| file.held)
            .collect()
    }

    fn present(&self, at: &Location) -> Option<[u8; 32]> {
        self.fs.metadata(at).ok()?;
        self.fs.content_hash(at, &CancelToken::new()).ok()
    }

    fn kept_old(&self) -> bool {
        self.held()
            .iter()
            .any(|at| self.present(at) == Some(OLD_HASH))
    }

    fn state(&self) -> JobState {
        lock(&self.journal).read_job(SAVE).unwrap().0
    }

    fn restore(&self) -> Result<PreviousVersion, RestoreError> {
        self.holds.restore_previous(SAVE, &self.ops())
    }
}

const OFF: &str = "";
const SAVED: Option<(u64, Option<i64>, [u8; 32])> = Some((11, Some(2_000), NEW_HASH));
const ORIGINAL: Option<(u64, Option<i64>, [u8; 32])> = Some((3, Some(1_000), OLD_HASH));

#[test]
fn a_save_outside_the_session_still_releases_its_old_file_at_finish() {
    let session = Session::new(OFF);
    session.save_with(&profile_factory(
        Arc::clone(&session.journal),
        session.dirs.clone(),
    ));
    assert_eq!(session.file(TARGET), SAVED);
    assert!(session.held().is_empty());
    assert!(lock(&session.journal).held(SAVE).unwrap().is_empty());
    assert_eq!(session.restore(), Err(RestoreError::NotKept));
}

#[test]
fn a_compare_save_keeps_its_old_file_after_finish() {
    let session = Session::new(OFF);
    session.save();
    assert_eq!(session.state(), JobState::Done);
    assert_eq!(session.file(TARGET), SAVED);
    assert!(session.kept_old());
    assert_eq!(lock(&session.journal).held(SAVE).unwrap().len(), 1);
}

#[test]
fn restoring_puts_the_exact_old_file_back_once() {
    let session = Session::new(OFF);
    session.save();
    assert_eq!(
        session.restore(),
        Ok(PreviousVersion::Undone {
            at: session.at(TARGET)
        })
    );
    assert_eq!(session.file(TARGET), ORIGINAL);
    assert_eq!(session.state(), JobState::Undone);
    assert_eq!(session.restore(), Err(RestoreError::NotKept));
    session.holds.release(session.fs.as_ref());
    assert_eq!(session.file(TARGET), ORIGINAL);
    assert!(lock(&session.journal).held(SAVE).unwrap().is_empty());
    assert!(
        session
            .held()
            .iter()
            .all(|at| session.present(at).is_none())
    );
}

#[test]
fn restoring_after_an_outside_edit_is_refused_and_keeps_both_versions() {
    for edited in [(11, 2_000), (5, 9_000)] {
        let session = Session::new(OFF);
        session.save();
        session.fs.remove_file(&session.at(TARGET)).unwrap();
        session.put(TARGET, edited, [9; 32]);
        assert_eq!(session.restore(), Err(RestoreError::Changed), "{edited:?}");
        assert_eq!(
            session.file(TARGET),
            Some((edited.0, Some(edited.1), [9; 32])),
            "{edited:?}"
        );
        assert!(session.kept_old(), "{edited:?}");
        assert_eq!(session.state(), JobState::Done, "{edited:?}");
        assert!(!super::super::held::is_restoring(&session.journal, SAVE).unwrap());
        assert_eq!(lock(&session.journal).held(SAVE).unwrap().len(), 1);
        let old = session.held();
        session.holds.release(session.fs.as_ref());
        assert!(old.iter().all(|at| session.present(at).is_none()));
        let report = crate::jobs::recover_profile(
            &session.journal,
            session.fs.as_ref(),
            (&session.dirs.data, &[]),
            false,
        )
        .unwrap();
        assert!(report.pending.is_empty());
        assert_eq!(
            session.file(TARGET),
            Some((edited.0, Some(edited.1), [9; 32]))
        );
    }
}

#[test]
fn exit_releases_the_session_holds() {
    let session = Session::new(OFF);
    session.save();
    let held = session.held();
    session.holds.release(session.fs.as_ref());
    assert_eq!(session.file(TARGET), SAVED);
    assert!(held.iter().all(|at| session.present(at).is_none()));
    assert!(lock(&session.journal).held(SAVE).unwrap().is_empty());
    assert_eq!(session.restore(), Err(RestoreError::NotKept));
}

#[test]
fn exit_puts_the_old_file_back_when_the_saved_file_is_gone() {
    let session = Session::new(OFF);
    session.save();
    session.fs.remove_file(&session.at(TARGET)).unwrap();
    session.holds.release(session.fs.as_ref());
    assert_eq!(session.file(TARGET), ORIGINAL);
}

#[test]
fn start_after_a_crash_with_session_holds_loses_nothing() {
    for saved_gone in [false, true] {
        let session = Session::new(OFF);
        session.save();
        if saved_gone {
            session.fs.remove_file(&session.at(TARGET)).unwrap();
        }
        recover(&session.journal, session.fs.as_ref()).unwrap();
        let expected = if saved_gone { ORIGINAL } else { SAVED };
        assert_eq!(session.file(TARGET), expected, "saved gone {saved_gone}");
        assert!(
            session
                .held()
                .iter()
                .all(|at| session.present(at).is_none())
        );
        assert!(lock(&session.journal).restore_pending().unwrap().is_empty());
    }
}

#[test]
fn start_after_a_crash_following_a_restore_keeps_the_restored_file() {
    let session = Session::new(OFF);
    session.save();
    assert!(session.restore().is_ok());
    recover(&session.journal, session.fs.as_ref()).unwrap();
    assert_eq!(session.file(TARGET), ORIGINAL);
    assert!(lock(&session.journal).held(SAVE).unwrap().is_empty());
}

fn stranded_restore(session: &Session) {
    session.save();
    let old = session.held().remove(0);
    let busy = || Fault::Fail(FsErrorKind::SharingViolation);
    let text = old.path().to_text();
    session.fs.fault(&text, FakeOp::Rename, 1, busy());
    session
        .fs
        .fault(&format!("{text}.current"), FakeOp::Rename, 1, busy());
    assert_eq!(
        session.restore(),
        Err(RestoreError::Fs(FsErrorKind::SharingViolation))
    );
}

#[test]
fn a_crash_in_the_middle_of_a_restore_keeps_both_versions_at_exit_and_start() {
    let session = Session::new(OFF);
    stranded_restore(&session);
    let versions = |session: &Session| {
        let mut found: Vec<_> = session
            .held()
            .iter()
            .filter_map(|at| session.present(at))
            .collect();
        found.sort_unstable();
        found
    };
    assert_eq!(versions(&session), [OLD_HASH, NEW_HASH]);
    assert_eq!(session.state(), JobState::RestorePending);
    session.holds.release(session.fs.as_ref());
    assert_eq!(versions(&session), [OLD_HASH, NEW_HASH]);
    recover(&session.journal, session.fs.as_ref()).unwrap();
    assert_eq!(versions(&session), [OLD_HASH, NEW_HASH]);
    assert_eq!(session.state(), JobState::RestorePending);
}
