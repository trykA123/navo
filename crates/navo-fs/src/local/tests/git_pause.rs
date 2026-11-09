//! A file operation stops a running Git child: the Windows proof with its plain-rename
//! control, and Unix job-wide pause tests.

use std::fs;
use std::path::Path;
use std::process::Command;
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(unix)]
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use navo_core::{ConflictPolicy, FsError, OpKind, PlanRequest, plan};
#[cfg(unix)]
use navo_core::{OperationPlan, ProviderId, Step, Totals};
#[cfg(unix)]
use navo_git::cli::stop_under;
use navo_git::{Cancel, CliGit, GitError, ProcessOptions, ProcessOutput};

#[cfg(unix)]
use crate::run_job;
use crate::{
    CancelToken, HeldFile, JobContext, JobLog, JobOutcome, ProviderOps, StepOutcome,
    run_job_parallel, scan,
};
#[cfg(unix)]
use crate::{Capabilities, ListSink, Provider};

use super::*;

/// A running alias child; dropping it cancels the call and joins its thread.
struct WaitingGit {
    cancel: Cancel,
    handle: Option<thread::JoinHandle<Result<ProcessOutput, GitError>>>,
}

impl WaitingGit {
    /// Cancels the call if it still runs and returns its result.
    fn stop(mut self) -> Result<ProcessOutput, GitError> {
        self.cancel.cancel();
        self.handle.take().unwrap().join().unwrap()
    }
}

impl Drop for WaitingGit {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// Starts `repo` with a `navo-wait` alias whose shell writes `started` and then sleeps.
fn repo_with_alias(repo: &Path, started: &Path) {
    fs::create_dir(repo).unwrap();
    run_git(repo, &["init", "-q"]);
    let started = started.to_string_lossy().replace('\\', "/");
    let alias = format!("!echo started > \"{started}\"; sleep 10");
    run_git(repo, &["config", "alias.navo-wait", &alias]);
}

/// Runs one Git command in `repo` and fails the test when it exits non-zero.
fn run_git(repo: &Path, args: &[&str]) {
    let output = git_command()
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

/// The platform's Git executable.
fn git_command() -> Command {
    Command::new(if cfg!(windows) { "git.exe" } else { "git" })
}

/// Runs the waiting alias on a thread and returns its guarded handle.
fn start_waiting_git(repo: &Path) -> WaitingGit {
    let git = CliGit::new(repo).unwrap();
    let options = ProcessOptions {
        stdout_cap: 64 * 1024,
        timeout: Duration::from_secs(30),
    };
    let cancel = Cancel::default();
    let token = cancel.clone();
    let handle = thread::spawn(move || git.run_cancellable(&["navo-wait"], &token, &options));
    WaitingGit {
        cancel,
        handle: Some(handle),
    }
}

/// Waits until the alias wrote `started`.
fn wait_started(started: &Path) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !started.exists() {
        assert!(Instant::now() < deadline, "the Git alias never started");
        thread::sleep(Duration::from_millis(5));
    }
}

/// Joins the waiting Git call and checks it was stopped as Paused within one second.
fn paused_within(child: WaitingGit, began: Instant) {
    let result = child.stop();
    let took = began.elapsed();
    assert!(
        matches!(result, Err(GitError::Paused)),
        "the paused file operation must stop the Git child: {result:?}"
    );
    assert!(took < Duration::from_secs(1), "the stop took {took:?}");
}

/// A quiet log for a delete job.
struct Silent;

impl JobLog for Silent {
    fn step(&mut self, _seq: u32, _outcome: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn undone(&mut self, _seq: u32, _outcome: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn journal_held(&mut self, _held: &HeldFile) -> Result<bool, FsError> {
        Ok(false)
    }
}

/// The delete plan of one repository.
fn delete_plan(repo: &Path) -> navo_core::OperationPlan {
    let items = scan(&LocalFs::new(), &[location_of(repo)], &CancelToken::new()).unwrap();
    plan(&PlanRequest {
        kind: OpKind::Delete,
        items,
        target: None,
        existing: vec![],
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: vec![],
    })
}

/// Runs a delete plan and returns its outcome.
fn delete_outcome(ops: &dyn ProviderOps, plan: &navo_core::OperationPlan) -> JobOutcome {
    let cancel = CancelToken::new();
    let mut log = Silent;
    let mut progress = |_: &crate::Progress| {};
    let mut ctx = JobContext {
        ops,
        log: &mut log,
        cancel: &cancel,
        progress: &mut progress,
    };
    run_job_parallel(plan, &[], &mut ctx, 2)
}

/// Counts job-level pauses and notices a remove that ran before one.
#[cfg(unix)]
struct Counting<'a> {
    fs: &'a LocalFs,
    pauses: AtomicUsize,
    first_remove_pauses: AtomicUsize,
    removed_before_pause: AtomicBool,
    pause_on_copy: bool,
}

#[cfg(unix)]
impl Counting<'_> {
    /// Notes a remove call before the job-level pause, if it is the first.
    fn removing(&self) {
        let pauses = self.pauses.load(Ordering::SeqCst);
        if pauses == 0 {
            self.removed_before_pause.store(true, Ordering::SeqCst);
        }
        let _ = self.first_remove_pauses.compare_exchange(
            usize::MAX,
            pauses,
            Ordering::SeqCst,
            Ordering::SeqCst,
        );
    }
}

#[cfg(unix)]
impl Provider for Counting<'_> {
    fn id(&self) -> ProviderId {
        self.fs.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.fs.capabilities()
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.fs.list(folder, sink, cancel)
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.fs.metadata(location)
    }
}

#[cfg(unix)]
impl ProviderOps for Counting<'_> {
    fn pause_git(&self, location: &Location) -> Result<Option<crate::GitPauseGuard>, FsError> {
        self.pauses.fetch_add(1, Ordering::SeqCst);
        self.fs.pause_git(location)
    }

    fn create_folder(&self, location: &Location) -> Result<(), FsError> {
        self.fs.create_folder(location)
    }

    fn create_file(&self, location: &Location) -> Result<(), FsError> {
        self.fs.create_file(location)
    }

    fn copy_file(
        &self,
        source: &Location,
        target: &Location,
        progress: &mut dyn FnMut(u64),
        cancel: &CancelToken,
    ) -> Result<u64, FsError> {
        let mut first = true;
        self.fs.copy_file(
            source,
            target,
            &mut |bytes| {
                if self.pause_on_copy && first {
                    first = false;
                    cancel.pause();
                }
                progress(bytes);
            },
            cancel,
        )
    }

    fn rename(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.rename(source, target)
    }

    fn replace(&self, source: &Location, target: &Location) -> Result<(), FsError> {
        self.fs.replace(source, target)
    }

    fn remove_file(&self, location: &Location) -> Result<(), FsError> {
        self.removing();
        self.fs.remove_file(location)
    }

    fn remove_link(&self, location: &Location) -> Result<(), FsError> {
        self.removing();
        self.fs.remove_link(location)
    }

    fn remove_empty_folder(&self, location: &Location) -> Result<(), FsError> {
        self.removing();
        self.fs.remove_empty_folder(location)
    }

    fn recycle(&self, location: &Location) -> Result<(), FsError> {
        self.removing();
        self.fs.recycle(location)
    }
}

/// A move plan with one copy followed by removing the source file.
#[cfg(unix)]
fn move_plan(source: &Path, target: &Path, size: u64) -> OperationPlan {
    OperationPlan {
        kind: OpKind::Move,
        steps: vec![
            Step::CopyFile {
                source: location_of(source),
                target: location_of(target),
                size,
            },
            Step::RemoveFile {
                location: location_of(source),
            },
        ],
        totals: Totals {
            files: 1,
            folders: 0,
            bytes: size,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    }
}

/// Runs a move that the user pauses during the copy, then resumes.
#[cfg(unix)]
#[expect(clippy::too_many_lines, reason = "one pause and resume scenario")]
fn user_paused_move(parallel: bool) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("paused-move-repo");
    let target = dir.path().join("copied.bin");
    let started = dir.path().join("started-move.txt");
    repo_with_alias(&repo, &started);
    let bytes = 4 << 20;
    fs::write(repo.join("big.bin"), vec![b'x'; bytes]).unwrap();
    let plan = move_plan(&repo.join("big.bin"), &target, bytes as u64);
    let fs = LocalFs::new();
    let counting = Counting {
        fs: &fs,
        pauses: AtomicUsize::new(0),
        first_remove_pauses: AtomicUsize::new(usize::MAX),
        removed_before_pause: AtomicBool::new(false),
        pause_on_copy: true,
    };
    let cancel = CancelToken::new();
    let (paused, paused_rx) = mpsc::channel();
    let mut report = move |progress: &crate::Progress| {
        if progress.paused {
            let _ = paused.send(());
        }
    };
    let outcome = thread::scope(|scope| {
        let job = scope.spawn(|| {
            let mut log = Silent;
            let mut ctx = JobContext {
                ops: &counting,
                log: &mut log,
                cancel: &cancel,
                progress: &mut report,
            };
            if parallel {
                run_job_parallel(&plan, &[], &mut ctx, 2)
            } else {
                run_job(&plan, &[], &mut ctx)
            }
        });
        paused_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the copy pauses");
        let child = start_waiting_git(&repo);
        wait_started(&started);
        let pause = stop_under(&repo, Duration::from_secs(1)).unwrap();
        paused_within(child, Instant::now());
        drop(pause);
        cancel.resume();
        job.join().unwrap()
    });
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(fs::read(&target).unwrap().len(), bytes);
    assert_eq!(counting.pauses.load(Ordering::SeqCst), 2);
    assert_eq!(counting.first_remove_pauses.load(Ordering::SeqCst), 2);
}

#[cfg(unix)]
#[test]
fn a_paused_serial_copy_releases_git() {
    user_paused_move(false);
}

#[cfg(unix)]
#[test]
fn a_paused_parallel_copy_releases_git() {
    user_paused_move(true);
}

/// Runs a delete that the user pauses before its first step, then resumes.
#[cfg(unix)]
#[expect(clippy::too_many_lines, reason = "one pause and resume scenario")]
fn user_paused_delete(parallel: bool) {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("paused-job-repo");
    let started = dir.path().join("started-paused-job.txt");
    repo_with_alias(&repo, &started);
    let plan = delete_plan(&repo);
    let fs = LocalFs::new();
    let counting = Counting {
        fs: &fs,
        pauses: AtomicUsize::new(0),
        first_remove_pauses: AtomicUsize::new(usize::MAX),
        removed_before_pause: AtomicBool::new(false),
        pause_on_copy: false,
    };
    let cancel = CancelToken::new();
    cancel.pause();
    let (paused, paused_rx) = mpsc::channel();
    let mut report = move |progress: &crate::Progress| {
        if progress.paused {
            let _ = paused.send(());
        }
    };
    let outcome = thread::scope(|scope| {
        let job = scope.spawn(|| {
            let mut log = Silent;
            let mut ctx = JobContext {
                ops: &counting,
                log: &mut log,
                cancel: &cancel,
                progress: &mut report,
            };
            if parallel {
                run_job_parallel(&plan, &[], &mut ctx, 2)
            } else {
                run_job(&plan, &[], &mut ctx)
            }
        });
        paused_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("the job pauses");
        let child = start_waiting_git(&repo);
        wait_started(&started);
        let pause = stop_under(&repo, Duration::from_secs(1)).unwrap();
        paused_within(child, Instant::now());
        drop(pause);
        cancel.resume();
        job.join().unwrap()
    });
    assert_eq!(outcome, JobOutcome::Done);
    assert!(!repo.exists());
    assert_eq!(counting.pauses.load(Ordering::SeqCst), 2);
    assert_eq!(counting.first_remove_pauses.load(Ordering::SeqCst), 2);
    assert!(!counting.removed_before_pause.load(Ordering::SeqCst));
}

#[cfg(unix)]
#[test]
fn a_user_pause_releases_git_in_a_serial_job() {
    user_paused_delete(false);
}

#[cfg(unix)]
#[test]
fn a_user_pause_releases_git_in_a_parallel_job() {
    user_paused_delete(true);
}

#[cfg(unix)]
#[test]
fn a_rename_stops_a_running_git_child() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let moved = dir.path().join("repo-renamed");
    let started = dir.path().join("started.txt");
    repo_with_alias(&repo, &started);
    let child = start_waiting_git(&repo);
    wait_started(&started);

    let (source, target) = (location_of(&repo), location_of(&moved));
    let began = Instant::now();
    LocalFs::new().rename(&source, &target).unwrap();
    paused_within(child, began);
    assert!(moved.exists() && !repo.exists());
    fs::rename(&moved, &repo).unwrap();
}

#[cfg(unix)]
#[test]
fn a_rename_inside_a_pause_does_not_wait_for_git() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("paused-repo");
    let moved = dir.path().join("paused-repo-renamed");
    let started = dir.path().join("started-paused.txt");
    repo_with_alias(&repo, &started);
    let child = start_waiting_git(&repo);
    wait_started(&started);

    let pause = stop_under(&repo, Duration::from_secs(1)).unwrap();
    let (source, target) = (location_of(&repo), location_of(&moved));
    let began = Instant::now();
    LocalFs::new().rename_atomic(&source, &target).unwrap();
    assert!(
        began.elapsed() < Duration::from_millis(50),
        "the nested pause waited"
    );
    paused_within(child, began);
    drop(pause);
    fs::rename(&moved, &repo).unwrap();
}

#[cfg(unix)]
#[test]
fn a_delete_job_pauses_the_root_before_removing_the_repo() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let started = dir.path().join("started-delete.txt");
    repo_with_alias(&repo, &started);
    let plan = delete_plan(&repo);
    let fs = LocalFs::new();
    let counting = Counting {
        fs: &fs,
        pauses: AtomicUsize::new(0),
        first_remove_pauses: AtomicUsize::new(usize::MAX),
        removed_before_pause: AtomicBool::new(false),
        pause_on_copy: false,
    };
    let child = start_waiting_git(&repo);
    wait_started(&started);

    let began = Instant::now();
    let outcome = delete_outcome(&counting, &plan);
    paused_within(child, began);
    assert_eq!(outcome, JobOutcome::Done);
    assert!(!repo.exists());
    assert_eq!(counting.pauses.load(Ordering::SeqCst), 1);
    assert!(
        !counting.removed_before_pause.load(Ordering::SeqCst),
        "the job must pause before its first remove"
    );
}

#[cfg(windows)]
#[test]
fn a_rename_stops_a_running_git_child() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    let moved = dir.path().join("repo-renamed");
    let started = dir.path().join("started.txt");
    repo_with_alias(&repo, &started);
    let child = start_waiting_git(&repo);
    wait_started(&started);

    let control = fs::rename(&repo, &moved);
    assert!(
        control.is_err(),
        "the running Git child must hold its folder: {control:?}"
    );

    let (source, target) = (location_of(&repo), location_of(&moved));
    let began = Instant::now();
    LocalFs::new().rename(&source, &target).unwrap();
    paused_within(child, began);
    assert!(moved.exists() && !repo.exists());
    fs::rename(&moved, &repo).unwrap();
}

#[cfg(windows)]
#[test]
fn a_recycle_stops_a_running_git_child() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("recycled-repo");
    let moved = dir.path().join("recycled-repo-renamed");
    let started = dir.path().join("started-recycle.txt");
    repo_with_alias(&repo, &started);
    let child = start_waiting_git(&repo);
    wait_started(&started);

    let control = fs::rename(&repo, &moved);
    assert!(
        control.is_err(),
        "the running Git child must hold its folder: {control:?}"
    );

    let location = location_of(&repo);
    let began = Instant::now();
    LocalFs::new().recycle(&location).unwrap();
    paused_within(child, began);
    assert!(!repo.exists(), "the recycled folder must be gone");
    // The VM Recycle Bin gains one item per run; the test leaves it there.
}

#[cfg(windows)]
#[test]
fn a_delete_job_removes_a_repo_with_a_running_git_child() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("deleted-repo");
    let started = dir.path().join("started-delete.txt");
    repo_with_alias(&repo, &started);
    let plan = delete_plan(&repo);
    let child = start_waiting_git(&repo);
    wait_started(&started);

    let began = Instant::now();
    let outcome = delete_outcome(&LocalFs::new(), &plan);
    paused_within(child, began);
    assert_eq!(outcome, JobOutcome::Done);
    assert!(!repo.exists());
}
