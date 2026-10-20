use std::path::PathBuf;

use rusty_core::{ConflictPolicy, OpKind, OperationPlan, PlanRequest, Step, plan};

use super::ops::child;
use super::*;
use crate::job::{
    HeldFile, JobContext, JobLog, JobOutcome, Journaled, StepOutcome, roll_back, run_job,
};
use crate::{existing_at, scan};

struct Linked {
    dir: TempDir,
    plan: OperationPlan,
    at: PathBuf,
}

#[cfg(windows)]
fn extended(path: &std::path::Path) -> PathBuf {
    PathBuf::from(format!(r"\\?\{}", path.display()))
}

#[cfg(not(windows))]
fn extended(path: &std::path::Path) -> PathBuf {
    path.to_path_buf()
}

fn linked_report() -> Linked {
    linked_report_in(&["links"])
}

fn linked_report_in(folders: &[&str]) -> Linked {
    let (dir, root) = temp();
    fs::write(dir.path().join("report.txt"), b"source contents").unwrap();
    let mut target = root.clone();
    let mut folder = dir.path().to_path_buf();
    for name in folders {
        target = child(&target, name);
        folder.push(name);
    }
    fs::create_dir_all(extended(&folder)).unwrap();
    let ops = LocalFs::new();
    let cancel = CancelToken::new();
    let items = scan(&ops, &[child(&root, "report.txt")], &cancel).unwrap();
    let existing = existing_at(&ops, &items, &target, &cancel).unwrap();
    let plan = plan(&PlanRequest {
        kind: OpKind::Link,
        items,
        target: Some(target),
        existing,
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: Vec::new(),
    });
    let Some(Step::MakeLink { at, .. }) = plan.steps.first() else {
        panic!("a link plan makes a link: {plan:?}");
    };
    let at = PathBuf::from(at.as_str());
    Linked { dir, plan, at }
}

#[derive(Default)]
struct Steps(Vec<(u32, StepOutcome)>);

impl JobLog for Steps {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.0.push((seq, outcome));
        Ok(())
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.step(seq, outcome)
    }

    fn journal_held(&mut self, _: &HeldFile) -> Result<bool, FsError> {
        Ok(false)
    }
}

fn with_job(
    job: impl FnOnce(&mut JobContext<'_>) -> JobOutcome,
) -> (JobOutcome, Vec<(u32, StepOutcome)>) {
    let mut log = Steps::default();
    let outcome = job(&mut JobContext {
        ops: &LocalFs::new(),
        log: &mut log,
        cancel: &CancelToken::new(),
        progress: &mut |_| {},
    });
    (outcome, log.0)
}

fn run_link(plan: &OperationPlan) -> (JobOutcome, Vec<(u32, StepOutcome)>) {
    with_job(|ctx| run_job(plan, &[], ctx))
}

fn recover_link(plan: &OperationPlan) -> (JobOutcome, Vec<(u32, StepOutcome)>) {
    let job = Journaled {
        plan,
        done: &[],
        held: &[],
        placed: &[],
        held_unreadable: false,
        unsure: Some(0),
    };
    with_job(|ctx| roll_back(&job, ctx))
}

fn undo_link(plan: &OperationPlan) -> (JobOutcome, Vec<(u32, StepOutcome)>) {
    let job = Journaled {
        plan,
        done: &[0],
        held: &[],
        placed: &[],
        held_unreadable: false,
        unsure: None,
    };
    with_job(|ctx| roll_back(&job, ctx))
}

fn source_intact(linked: &Linked) -> bool {
    fs::read(linked.dir.path().join("report.txt")).unwrap() == b"source contents"
}

#[test]
fn a_link_job_makes_a_link_and_undo_removes_only_it() {
    let linked = linked_report();
    let (outcome, log) = run_link(&linked.plan);
    assert_eq!(
        (outcome, log),
        (JobOutcome::Done, vec![(0, StepOutcome::Done)])
    );
    #[cfg(unix)]
    assert_eq!(
        fs::read_link(&linked.at).unwrap(),
        linked.dir.path().join("report.txt")
    );
    #[cfg(windows)]
    assert!(
        linked
            .at
            .to_string_lossy()
            .ends_with("report.txt - Shortcut.lnk")
    );
    let (outcome, log) = undo_link(&linked.plan);
    assert_eq!(
        (outcome, log),
        (JobOutcome::Done, vec![(0, StepOutcome::Done)])
    );
    assert!(fs::symlink_metadata(&linked.at).is_err());
    assert!(source_intact(&linked));
    let left: Vec<_> = fs::read_dir(linked.dir.path().join("links"))
        .unwrap()
        .collect();
    assert_eq!(left.len(), 0);
}

#[test]
fn a_name_taken_at_run_time_fails_and_keeps_the_file() {
    let linked = linked_report();
    fs::write(&linked.at, b"mine").unwrap();
    let (outcome, log) = run_link(&linked.plan);
    assert!(
        matches!(outcome, JobOutcome::Failed { seq: 0, .. }),
        "{outcome:?}"
    );
    assert_eq!(log, [(0, StepOutcome::Failed)]);
    assert_eq!(fs::read(&linked.at).unwrap(), b"mine");
    assert_eq!(
        fs::read_dir(linked.dir.path().join("links"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn undo_keeps_a_folder_that_replaced_the_link() {
    let linked = linked_report();
    assert_eq!(run_link(&linked.plan).0, JobOutcome::Done);
    fs::remove_file(&linked.at).unwrap();
    fs::create_dir(&linked.at).unwrap();
    fs::write(linked.at.join("inside.txt"), b"kept").unwrap();
    let (outcome, log) = undo_link(&linked.plan);
    assert_eq!(
        (outcome, log),
        (JobOutcome::Done, vec![(0, StepOutcome::Skipped)])
    );
    assert_eq!(fs::read(linked.at.join("inside.txt")).unwrap(), b"kept");
}

#[cfg(unix)]
#[test]
fn undo_keeps_a_regular_file_that_replaced_the_symlink() {
    let linked = linked_report();
    assert_eq!(run_link(&linked.plan).0, JobOutcome::Done);
    fs::remove_file(&linked.at).unwrap();
    fs::write(&linked.at, b"mine").unwrap();
    let (outcome, log) = undo_link(&linked.plan);
    assert_eq!(
        (outcome, log),
        (JobOutcome::Done, vec![(0, StepOutcome::Skipped)])
    );
    assert_eq!(fs::read(&linked.at).unwrap(), b"mine");
}

#[cfg(unix)]
#[test]
fn undo_removes_a_dangling_symlink_it_made() {
    let linked = linked_report();
    assert_eq!(run_link(&linked.plan).0, JobOutcome::Done);
    fs::remove_file(linked.dir.path().join("report.txt")).unwrap();
    let (outcome, log) = undo_link(&linked.plan);
    assert_eq!(
        (outcome, log),
        (JobOutcome::Done, vec![(0, StepOutcome::Done)])
    );
    assert!(fs::symlink_metadata(&linked.at).is_err());
}

#[cfg(unix)]
#[test]
fn undo_keeps_a_symlink_that_replaced_ours_with_another_target() {
    let linked = linked_report();
    assert_eq!(run_link(&linked.plan).0, JobOutcome::Done);
    let other = linked.dir.path().join("other.txt");
    fs::write(&other, b"other").unwrap();
    fs::remove_file(&linked.at).unwrap();
    std::os::unix::fs::symlink(&other, &linked.at).unwrap();
    let (outcome, log) = undo_link(&linked.plan);
    assert_eq!(
        (outcome, log),
        (JobOutcome::Done, vec![(0, StepOutcome::Skipped)])
    );
    assert_eq!(fs::read_link(&linked.at).unwrap(), other);
}

#[cfg(unix)]
#[test]
fn recovery_keeps_a_symlink_someone_else_made_at_an_unsure_name() {
    let linked = linked_report();
    let other = linked.dir.path().join("other.txt");
    fs::write(&other, b"other").unwrap();
    std::os::unix::fs::symlink(&other, &linked.at).unwrap();
    let (outcome, log) = recover_link(&linked.plan);
    assert_eq!(
        (outcome, log),
        (JobOutcome::Done, vec![(0, StepOutcome::Skipped)])
    );
    assert_eq!(fs::read_link(&linked.at).unwrap(), other);
}

#[cfg(unix)]
#[test]
fn recovery_removes_the_symlink_a_crashed_job_made() {
    let linked = linked_report();
    assert_eq!(run_link(&linked.plan).0, JobOutcome::Done);
    let (outcome, log) = recover_link(&linked.plan);
    assert_eq!(
        (outcome, log),
        (JobOutcome::Done, vec![(0, StepOutcome::Done)])
    );
    assert!(fs::symlink_metadata(&linked.at).is_err());
}

#[cfg(windows)]
#[test]
fn a_shortcut_in_a_folder_path_over_260_characters_is_made_and_undone() {
    let segment = "a-folder-name-of-forty-characters-long--";
    let linked = linked_report_in(&[segment; 8]);
    assert!(linked.at.as_os_str().len() > 260);
    assert_eq!(run_link(&linked.plan).0, JobOutcome::Done);
    let ops = LocalFs::new();
    let at = location_of(&linked.at);
    assert!(
        crate::ProviderOps::link_target(&ops, &at)
            .unwrap()
            .is_some()
    );
    let (outcome, log) = undo_link(&linked.plan);
    assert_eq!(
        (outcome, log),
        (JobOutcome::Done, vec![(0, StepOutcome::Done)])
    );
    assert_eq!(
        crate::Provider::metadata(&ops, &at).unwrap_err().kind,
        rusty_core::FsErrorKind::NotFound
    );
}
