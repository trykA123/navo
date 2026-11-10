//! Tests for the plan sheet and one-job copy of many matching changes.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use navo_compare::model::CancelToken;
use navo_compare::text::{Direction, Options};
use navo_core::{Location, ProviderPath};
use navo_fs::{CancelToken as FsCancel, JobContext, JobOutcome, LocalFs};
use navo_store::JobId;
use navo_ui::browser::LogFactory;
use navo_ui::compare_diff::{HunkView, PairRequest};

use crate::compare::diff::convert::to_pair_text;
use crate::compare::diff::copy::{CopyContext, CopyRefusal, CopyRequest};
use crate::compare::diff::copy_many::plan::plan_sheet;
use crate::compare::diff::copy_many::run_sheet;
use crate::compare::diff::load::{LoadInput, load_pair};
use crate::compare::local::LocalSide;
use crate::jobs::{SessionHolds, lock, open};

const LEFT: [&[u8]; 3] = [b"a\nb\nc\n", b"x\ny\nz\n", b"m\nn\no\n"];
const RIGHT: [&[u8]; 3] = [b"a\nB\nc\n", b"x\nY\nz\n", b"m\nN\no\n"];

struct Fixture {
    work: tempfile::TempDir,
    _store: tempfile::TempDir,
    journal: crate::jobs::SharedJournal,
    factory: LogFactory,
    ops: LocalFs,
}

impl Fixture {
    /// A work folder with three left/right text pairs and a compare job factory.
    fn new() -> Self {
        let work = tempfile::tempdir().unwrap();
        for index in 0..3 {
            fs::write(work.path().join(format!("left{index}.txt")), LEFT[index]).unwrap();
            fs::write(work.path().join(format!("right{index}.txt")), RIGHT[index]).unwrap();
        }
        let store = tempfile::tempdir().unwrap();
        let dirs = navo_store::StoreDirs::in_profile(store.path());
        dirs.ensure().unwrap();
        let journal = open(&dirs.journal_file()).unwrap();
        let factory = SessionHolds::new(Arc::clone(&journal), dirs).compare_factory(None);
        Self {
            work,
            _store: store,
            journal,
            factory,
            ops: LocalFs::new(),
        }
    }

    /// The path of one side of pair `index`.
    fn path(&self, index: usize, side: &str) -> PathBuf {
        self.work.path().join(format!("{side}{index}.txt"))
    }

    /// Read one side of a pair.
    fn read(&self, index: usize, side: &str) -> Vec<u8> {
        fs::read(self.path(index, side)).unwrap()
    }

    /// The names in the work folder, sorted.
    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.work.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// No staged `.navo-part` file is left behind.
    fn assert_no_staged(&self) {
        let names = self.names();
        assert!(
            names.iter().all(|name| !name.ends_with(".navo-part")),
            "{names:?}"
        );
    }

    /// The request the UI sends for the first change of pair `index`.
    fn request(&self, index: usize) -> CopyRequest {
        self.request_hunk(index, 0)
    }

    /// The request the UI sends for change `change` of pair `index`.
    fn request_hunk(&self, index: usize, change: usize) -> CopyRequest {
        let hunk = self.hunk(index, change);
        CopyRequest {
            pair: PairRequest {
                id: u64::try_from(index).unwrap(),
                path: format!("file{index}.txt"),
                left: Some(location(&self.path(index, "left"))),
                right: Some(location(&self.path(index, "right"))),
            },
            left_bytes: hunk.left_bytes,
            right_bytes: hunk.right_bytes,
            digest: hunk.digest.unwrap(),
            direction: Direction::LeftToRight,
            lines: None,
        }
    }

    /// Change `change` shown for pair `index` as the pair is now.
    fn hunk(&self, index: usize, change: usize) -> HunkView {
        let input = LoadInput {
            left: Some(side(self.work.path(), &format!("left{index}.txt"))),
            right: Some(side(self.work.path(), &format!("right{index}.txt"))),
            options: Options::default(),
            fallback: encoding_rs::UTF_8,
        };
        let loaded = load_pair(&input, &CancelToken::new());
        to_pair_text(&loaded).unwrap().hunks[change].clone()
    }

    /// The copy context for these tests.
    fn context<'a>(&'a self, options: &'a Options, cancel: &'a FsCancel) -> CopyContext<'a> {
        CopyContext {
            options,
            fallback: encoding_rs::UTF_8,
            ops: &self.ops,
            factory: &self.factory,
            cancel,
        }
    }

    /// Undo the newest compare Save job through the same journal.
    fn undo(&self) {
        let hooks = crate::rename_hooks::hooks(Some(Arc::clone(&self.journal)), None);
        let offer = (hooks.last)().unwrap().expect("a save is offered");
        let job = JobId(i64::try_from(offer.job).unwrap());
        let mut log = crate::jobs::undo_log(&self.journal, job).unwrap();
        let cancel = FsCancel::new();
        let outcome = navo_ui::browser::run_undo(
            &offer.plan,
            &offer.done,
            &mut JobContext {
                ops: &self.ops,
                log: log.as_mut(),
                cancel: &cancel,
                progress: &mut |_| {},
            },
        )
        .0;
        assert_eq!(outcome, JobOutcome::Done);
        log.finish(&outcome, &self.ops).unwrap();
    }
}

/// A local location from a filesystem path.
fn location(path: &Path) -> Location {
    Location::local(ProviderPath::from_text(path.to_str().unwrap())).unwrap()
}

/// One side of a pair, as the loader wants it.
fn side(root: &Path, name: &str) -> crate::compare::diff::load::Side {
    use navo_compare::model::{EntryKind, RelPath, SideEntry};
    let size = fs::metadata(root.join(name)).unwrap().len();
    let source = Arc::new(LocalSide::new(location(root)).unwrap());
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text(name)),
        EntryKind::File,
        size,
        None,
    );
    (source, entry)
}

#[test]
fn a_sheet_lists_every_file_and_refuses_one_without_staging() {
    let fixture = Fixture::new();
    let options = Options::default();
    let cancel = FsCancel::new();
    let ctx = fixture.context(&options, &cancel);
    let mut requests = vec![fixture.request(0), fixture.request(1), fixture.request(2)];
    requests[2].pair.right = None;
    let sheet = plan_sheet(&requests, &ctx);
    assert_eq!(sheet.files.len(), 3);
    let accepted: Vec<_> = sheet
        .files
        .iter()
        .filter(|file| file.refusal.is_none())
        .collect();
    assert_eq!(accepted.len(), 2);
    assert_eq!(sheet.files[2].refusal, Some("compare-copy-not-copyable"));
    assert_eq!(sheet.files[0].hunks, 1);
    assert!(sheet.files[0].bytes > 0);
    assert_eq!(fixture.read(0, "right"), RIGHT[0]);
    fixture.assert_no_staged();
}

#[test]
fn a_run_writes_the_accepted_files_and_undo_restores_both() {
    let fixture = Fixture::new();
    let options = Options::default();
    let cancel = FsCancel::new();
    let ctx = fixture.context(&options, &cancel);
    let mut requests = vec![fixture.request(0), fixture.request(1), fixture.request(2)];
    requests[2].pair.right = None;
    let sheet = plan_sheet(&requests, &ctx);
    let started = run_sheet(&sheet, &requests, &ctx).unwrap();
    assert_eq!(started.files, 2);
    assert_eq!(fixture.read(0, "right"), LEFT[0]);
    assert_eq!(fixture.read(1, "right"), LEFT[1]);
    assert_eq!(fixture.read(2, "right"), RIGHT[2]);
    fixture.assert_no_staged();
    let finished = lock(&fixture.journal).last_undoable("save").unwrap();
    assert_eq!(finished.expect("a save job").steps.len(), 2);
    fixture.undo();
    assert_eq!(fixture.read(0, "right"), RIGHT[0]);
    assert_eq!(fixture.read(1, "right"), RIGHT[1]);
    assert_eq!(fixture.read(2, "right"), RIGHT[2]);
}

#[test]
fn a_target_changed_between_the_sheet_and_the_run_is_refused_before_any_write() {
    let fixture = Fixture::new();
    let options = Options::default();
    let cancel = FsCancel::new();
    let ctx = fixture.context(&options, &cancel);
    let requests = vec![fixture.request(0), fixture.request(1)];
    let sheet = plan_sheet(&requests, &ctx);
    fs::write(fixture.path(1, "right"), b"x\nY\nz\nlate\n").unwrap();
    let changed = fixture.read(1, "right");
    assert_eq!(
        run_sheet(&sheet, &requests, &ctx),
        Err(CopyRefusal::Changed)
    );
    assert_eq!(fixture.read(0, "right"), RIGHT[0]);
    assert_eq!(fixture.read(1, "right"), changed);
    fixture.assert_no_staged();
    assert!(
        lock(&fixture.journal)
            .last_undoable("save")
            .unwrap()
            .is_none()
    );
}

#[test]
fn a_job_refusal_removes_every_staged_file() {
    let fixture = Fixture::new();
    let options = Options::default();
    let cancel = FsCancel::new();
    let requests = vec![fixture.request(0), fixture.request(1)];
    let sheet = plan_sheet(&requests, &fixture.context(&options, &cancel));
    let inner = Arc::clone(&fixture.factory);
    let target = fixture.path(1, "right");
    let racing: LogFactory = Arc::new(move |plan| {
        fs::write(&target, b"late\n").unwrap();
        inner(plan)
    });
    let ctx = CopyContext {
        options: &options,
        fallback: encoding_rs::UTF_8,
        ops: &fixture.ops,
        factory: &racing,
        cancel: &cancel,
    };
    assert_eq!(
        run_sheet(&sheet, &requests, &ctx),
        Err(CopyRefusal::Changed)
    );
    assert_eq!(fixture.read(0, "right"), RIGHT[0]);
    assert_eq!(fixture.read(1, "right"), b"late\n");
    fixture.assert_no_staged();
}

#[test]
fn two_requests_on_one_pair_become_one_file_with_both_changes() {
    let fixture = Fixture::new();
    fs::write(fixture.path(0, "left"), b"a\nb\nc\nd\ne\n").unwrap();
    fs::write(fixture.path(0, "right"), b"a\nB\nc\nD\ne\n").unwrap();
    let options = Options::default();
    let cancel = FsCancel::new();
    let ctx = fixture.context(&options, &cancel);
    let requests = vec![fixture.request_hunk(0, 0), fixture.request_hunk(0, 1)];
    let sheet = plan_sheet(&requests, &ctx);
    assert_eq!(sheet.files.len(), 1);
    assert_eq!(sheet.files[0].hunks, 2);
    run_sheet(&sheet, &requests, &ctx).unwrap();
    assert_eq!(fixture.read(0, "right"), b"a\nb\nc\nd\ne\n");
}
