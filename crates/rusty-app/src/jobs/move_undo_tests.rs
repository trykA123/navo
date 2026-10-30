use std::path::PathBuf;
use std::sync::Arc;

use rusty_core::{ConflictPolicy, Location, OpKind, OperationPlan, PlanRequest, plan};
use rusty_fs::{CancelToken, JobContext, JobOutcome, LocalFs, existing_at, run_job_parallel, scan};
use rusty_store::{JobId, JobState, StepState};
use rusty_ui::browser::{RenameReport, UndoOffer, run_undo};

use super::{SharedJournal, lock, log_factory, open, recover};

mod durable;
mod recovery;

struct Tree {
    dir: tempfile::TempDir,
    fs: LocalFs,
    journal: SharedJournal,
}

impl Tree {
    fn new(files: &[(&str, &str)]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        for folder in ["src", "dst", "aside"] {
            std::fs::create_dir(dir.path().join(folder)).unwrap();
        }
        let journal = open(&dir.path().join("navo.db")).unwrap();
        let tree = Self {
            dir,
            fs: LocalFs::new(),
            journal,
        };
        for (path, text) in files {
            tree.write(path, text);
        }
        tree
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }

    fn location(&self, relative: &str) -> Location {
        crate::startup::path_location(&self.path(relative)).unwrap()
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.path(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn read(&self, relative: &str) -> Option<String> {
        std::fs::read_to_string(self.path(relative)).ok()
    }

    fn replace(&self, relative: &str, kept_as: &str, text: &str) {
        std::fs::rename(self.path(relative), self.path(kept_as)).unwrap();
        self.write(relative, text);
    }

    fn plan(&self, names: &[&str], same_volume: bool, policy: ConflictPolicy) -> OperationPlan {
        let cancel = CancelToken::new();
        let sources: Vec<Location> = names
            .iter()
            .map(|name| self.location(&format!("src/{name}")))
            .collect();
        let items = scan(&self.fs, &sources, &cancel).unwrap();
        let folder = self.location("dst");
        let existing = existing_at(&self.fs, &items, &folder, &cancel).unwrap();
        plan(&PlanRequest {
            kind: OpKind::Move,
            items,
            target: Some(folder),
            existing,
            policy,
            same_volume,
            protected: Vec::new(),
        })
    }

    fn run(&self, plan: &OperationPlan, workers: usize) -> JobOutcome {
        let mut log = log_factory(Arc::clone(&self.journal))(plan).unwrap();
        let outcome = run_job_parallel(
            plan,
            &[],
            &mut JobContext {
                ops: &self.fs,
                log: log.as_mut(),
                cancel: &CancelToken::new(),
                progress: &mut |_| {},
            },
            workers,
        );
        log.finish(&outcome, &self.fs).unwrap();
        outcome
    }

    fn moved(&self, names: &[&str], workers: usize) -> JobOutcome {
        let plan = self.plan(names, true, ConflictPolicy::Ask);
        self.run(&plan, workers)
    }

    fn hooks(&self) -> rusty_ui::browser::RenameHooks {
        crate::rename_hooks::hooks(Some(Arc::clone(&self.journal)), None)
    }

    fn offer(&self) -> Option<UndoOffer> {
        (self.hooks().last)().unwrap()
    }

    fn undo(&self, offer: &UndoOffer) -> (JobOutcome, RenameReport) {
        let mut log = (self.hooks().undo_log)(offer).unwrap();
        let result = run_undo(
            &offer.plan,
            &offer.done,
            &mut JobContext {
                ops: &self.fs,
                log: log.as_mut(),
                cancel: &CancelToken::new(),
                progress: &mut |_| {},
            },
        );
        log.finish(&result.0, &self.fs).unwrap();
        result
    }

    fn reopened(self) -> Self {
        let Self { dir, fs, journal } = self;
        drop(journal);
        let journal = open(&dir.path().join("navo.db")).unwrap();
        Self { dir, fs, journal }
    }

    fn state(&self, job: u64) -> JobState {
        lock(&self.journal)
            .read_job(JobId(i64::try_from(job).unwrap()))
            .unwrap()
            .0
    }
}

const FILES: [(&str, &str); 3] = [
    ("src/a.txt", "alpha"),
    ("src/b.txt", "bravo!"),
    ("src/sub/c.txt", "charlie"),
];

fn present(tree: &Tree, paths: &[&str]) -> Vec<Option<String>> {
    paths.iter().map(|path| tree.read(path)).collect()
}

fn some(texts: &[&str]) -> Vec<Option<String>> {
    texts.iter().map(|text| Some((*text).to_owned())).collect()
}

#[test]
fn a_full_move_undo_puts_every_item_back_from_either_runner() {
    for workers in [1, 4] {
        let tree = Tree::new(&FILES);
        assert_eq!(
            tree.moved(&["a.txt", "b.txt", "sub"], workers),
            JobOutcome::Done
        );
        assert_eq!(tree.read("dst/sub/c.txt").as_deref(), Some("charlie"));
        let offer = tree.offer().expect("a same-volume move is offered");
        assert_eq!(offer.items(), 3);
        let (outcome, report) = tree.undo(&offer);
        assert_eq!((outcome, report.skipped), (JobOutcome::Done, 0));
        assert_eq!(
            present(&tree, &["src/a.txt", "src/b.txt", "src/sub/c.txt"]),
            some(&["alpha", "bravo!", "charlie"])
        );
        assert!(!tree.path("dst/a.txt").exists() && !tree.path("dst/sub").exists());
        assert_eq!(tree.state(offer.job), JobState::Undone);
        assert!(tree.offer().is_none());
    }
}

#[test]
fn a_partly_failed_move_undoes_only_the_steps_that_completed() {
    let tree = Tree::new(&FILES);
    let plan = tree.plan(&["a.txt", "b.txt", "sub"], true, ConflictPolicy::Ask);
    tree.write("dst/b.txt", "someone else");
    assert!(matches!(
        tree.run(&plan, 1),
        JobOutcome::Failed { seq: 1, .. }
    ));
    let offer = tree.offer().expect("the completed part is offered");
    assert_eq!(offer.done, vec![0]);
    assert_eq!(tree.undo(&offer).0, JobOutcome::Done);
    assert_eq!(
        present(
            &tree,
            &["src/a.txt", "src/b.txt", "src/sub/c.txt", "dst/b.txt"]
        ),
        some(&["alpha", "bravo!", "charlie", "someone else"])
    );
    assert!(!tree.path("dst/a.txt").exists());
}

#[test]
fn a_replaced_or_edited_destination_is_skipped_and_both_files_stay() {
    let tree = Tree::new(&FILES);
    assert_eq!(tree.moved(&["a.txt", "b.txt", "sub"], 1), JobOutcome::Done);
    tree.replace("dst/a.txt", "aside/a.txt", "a new file");
    std::fs::write(tree.path("dst/b.txt"), "bravo, edited").unwrap();
    let offer = tree.offer().unwrap();
    let (outcome, report) = tree.undo(&offer);
    assert_eq!((outcome, report.skipped), (JobOutcome::Done, 2));
    assert_eq!(
        present(
            &tree,
            &["dst/a.txt", "aside/a.txt", "dst/b.txt", "src/sub/c.txt"]
        ),
        some(&["a new file", "alpha", "bravo, edited", "charlie"])
    );
    assert!(!tree.path("src/a.txt").exists() && !tree.path("src/b.txt").exists());
}

#[test]
fn an_occupied_origin_is_skipped_and_never_overwritten() {
    let tree = Tree::new(&FILES);
    assert_eq!(tree.moved(&["a.txt"], 1), JobOutcome::Done);
    tree.write("src/a.txt", "made later");
    let offer = tree.offer().unwrap();
    assert_eq!(tree.undo(&offer).1.skipped, 1);
    assert_eq!(
        present(&tree, &["src/a.txt", "dst/a.txt"]),
        some(&["made later", "alpha"])
    );
}

#[test]
fn a_crash_after_the_undo_claim_recovers_without_moving_a_replacement() {
    let tree = Tree::new(&FILES);
    assert_eq!(tree.moved(&["a.txt", "b.txt", "sub"], 1), JobOutcome::Done);
    let offer = tree.offer().unwrap();
    drop((tree.hooks().undo_log)(&offer).unwrap());
    assert_eq!(tree.state(offer.job), JobState::RestorePending);
    tree.replace("dst/a.txt", "aside/a.txt", "a new file");
    let tree = tree.reopened();
    recover(&tree.journal, &tree.fs).unwrap();
    assert_eq!(
        present(
            &tree,
            &["dst/a.txt", "aside/a.txt", "src/b.txt", "src/sub/c.txt"]
        ),
        some(&["a new file", "alpha", "bravo!", "charlie"])
    );
    assert!(!tree.path("src/a.txt").exists());
    assert_eq!(tree.state(offer.job), JobState::RestorePending);
    assert_eq!(tree.offer().map(|offer| offer.done), Some(vec![0]));
}

#[test]
fn a_crash_mid_undo_recovers_the_rest_and_skips_what_is_already_back() {
    let tree = Tree::new(&FILES);
    assert_eq!(tree.moved(&["a.txt", "b.txt"], 1), JobOutcome::Done);
    let offer = tree.offer().unwrap();
    drop((tree.hooks().undo_log)(&offer).unwrap());
    std::fs::rename(tree.path("dst/b.txt"), tree.path("src/b.txt")).unwrap();
    let tree = tree.reopened();
    recover(&tree.journal, &tree.fs).unwrap();
    assert_eq!(
        present(&tree, &["src/a.txt", "src/b.txt"]),
        some(&["alpha", "bravo!"])
    );
    assert_eq!(tree.state(offer.job), JobState::RolledBack);
}

#[test]
fn a_cross_volume_move_is_not_offered() {
    let tree = Tree::new(&FILES);
    let plan = tree.plan(&["a.txt", "sub"], false, ConflictPolicy::Ask);
    assert_eq!(tree.run(&plan, 1), JobOutcome::Done);
    assert_eq!(tree.read("dst/a.txt").as_deref(), Some("alpha"));
    assert!(tree.offer().is_none());
}

#[test]
fn a_move_that_overwrote_a_file_is_not_offered() {
    let tree = Tree::new(&FILES);
    tree.write("dst/a.txt", "old");
    let plan = tree.plan(&["a.txt", "b.txt"], true, ConflictPolicy::OverwriteAll);
    assert_eq!(tree.run(&plan, 1), JobOutcome::Done);
    assert_eq!(tree.read("dst/a.txt").as_deref(), Some("alpha"));
    assert!(tree.offer().is_none());
}

#[test]
fn a_change_to_only_the_modified_time_is_skipped() {
    let tree = Tree::new(&FILES);
    assert_eq!(tree.moved(&["a.txt"], 1), JobOutcome::Done);
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
    std::fs::File::options()
        .write(true)
        .open(tree.path("dst/a.txt"))
        .unwrap()
        .set_modified(later)
        .unwrap();
    let offer = tree.offer().unwrap();
    assert_eq!(tree.undo(&offer).1.skipped, 1);
    assert_eq!(
        (tree.read("dst/a.txt"), tree.read("src/a.txt")),
        (Some("alpha".to_owned()), None)
    );
}

#[test]
fn a_moved_folder_goes_back_with_its_current_contents() {
    let tree = Tree::new(&FILES);
    assert_eq!(tree.moved(&["sub"], 1), JobOutcome::Done);
    tree.write("dst/sub/c.txt", "charlie, edited");
    tree.write("dst/sub/new.txt", "added later");
    let offer = tree.offer().unwrap();
    assert_eq!(
        tree.undo(&offer),
        (JobOutcome::Done, RenameReport::default())
    );
    assert_eq!(
        present(&tree, &["src/sub/c.txt", "src/sub/new.txt"]),
        some(&["charlie, edited", "added later"])
    );
    assert!(!tree.path("dst/sub").exists());
}

#[test]
fn a_cancelled_move_offers_and_undoes_only_what_it_moved() {
    let tree = Tree::new(&FILES);
    let plan = tree.plan(&["a.txt", "b.txt", "sub"], true, ConflictPolicy::Ask);
    let mut log = log_factory(Arc::clone(&tree.journal))(&plan).unwrap();
    let cancel = CancelToken::new();
    let outcome = rusty_fs::run_job(
        &plan,
        &[],
        &mut JobContext {
            ops: &tree.fs,
            log: log.as_mut(),
            cancel: &cancel,
            progress: &mut |_| cancel.cancel(),
        },
    );
    log.finish(&outcome, &tree.fs).unwrap();
    assert_eq!(outcome, JobOutcome::Cancelled);
    let offer = tree
        .offer()
        .expect("the moved part of a cancelled move is offered");
    assert!(!offer.done.is_empty() && offer.done.len() < 3);
    assert_eq!(tree.undo(&offer).0, JobOutcome::Done);
    assert_eq!(
        present(&tree, &["src/a.txt", "src/b.txt", "src/sub/c.txt"]),
        some(&["alpha", "bravo!", "charlie"])
    );
    assert!(
        std::fs::read_dir(tree.path("dst"))
            .unwrap()
            .next()
            .is_none()
    );
}
