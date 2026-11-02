use std::fs;
use std::process::Command;

use super::*;
use crate::git::{GitFs, location};

#[path = "../../../../../navo-git/tests/fixture/mod.rs"]
mod fixture;

fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .output()
        .unwrap()
        .status
        .success()
}

fn filtered_scene() -> (fixture::Fixture, Scene) {
    let fixture = fixture::Fixture::new().expect("Git is required for streaming copy tests");
    fixture.init(&["-b", "main"]);
    fixture.write(".gitattributes", b"data.bin filter=slow -text\n");
    fixture.write("data.bin", &vec![b'x'; 8 * 1024 * 1024]);
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("filtered copy");
    fixture.write(
        ".git/smudge.sh",
        b"echo \"$PPID\" > .git/filter.pid\nsleep 0.05\ncat\n",
    );
    fixture.git_ok(&["config", "filter.slow.smudge", "exec sh .git/smudge.sh"]);
    let mut scene = Scene::new(Feed::Whole, None);
    let source = location(fixture.repo(), "HEAD", "data.bin").unwrap();
    let Step::CopyFile {
        source: from, size, ..
    } = &mut scene.plan.steps[0]
    else {
        unreachable!();
    };
    *from = source;
    *size = 8 * 1024 * 1024;
    scene.plan.totals.bytes = *size;
    scene.sources = Sources::new().with(Arc::new(GitFs::new()));
    (fixture, scene)
}

#[test]
fn git_filtered_copy_reports_progress_and_cancel_reaps_the_child() {
    for workers in MODES {
        let (fixture, scene) = filtered_scene();
        let cancel = scene.cancel.clone();
        let mut observed = None;
        let mut progress = |update: &Progress| {
            if update.bytes_done == 0 || observed.is_some() {
                return;
            }
            let pid = fs::read_to_string(fixture.repo().join(".git/filter.pid")).unwrap();
            assert!(alive(pid.trim()), "progress must arrive while Git is alive");
            assert!(update.bytes_done < scene.plan.totals.bytes);
            observed = Some(pid);
            cancel.cancel();
        };
        let mut log = Log::default();
        let mut context = JobContext {
            ops: &LocalFs::new(),
            log: &mut log,
            cancel: &scene.cancel,
            progress: &mut progress,
        };
        let outcome = match workers {
            0 => run_job_with_sources(&scene.plan, &[], &mut context, &scene.sources),
            n => run_job_parallel_with_sources((&scene.plan, &scene.sources), &[], &mut context, n),
        };
        assert_eq!(outcome, JobOutcome::Cancelled);
        let pid = observed.expect("the copy must report bytes before cancellation");
        assert!(!alive(pid.trim()), "cancellation must reap Git");
        assert!(
            scene.names().is_empty(),
            "no partial target or temporary may remain"
        );
        assert!(log.held.is_empty());
    }
}

#[test]
fn git_filtered_copy_finishes_and_records_late_stderr() {
    let (fixture, mut scene) = filtered_scene();
    fixture.write(".git/smudge.sh", b"cat\necho late-filter-warning >&2\n");
    let git = Arc::new(GitFs::new());
    scene.sources = Sources::new().with(git.clone());
    let source = match &scene.plan.steps[0] {
        Step::CopyFile { source, .. } => source,
        _ => unreachable!(),
    };
    let outcome = scene.run(&mut Harness::default(), 0);
    assert_eq!(refused(&outcome), FsErrorKind::SourceFilterFailed);
    assert!(matches!(
        git.last_problem(source),
        Some(crate::git::GitProblem::Filter(_))
    ));
    assert!(scene.names().is_empty());
}

#[test]
fn git_filtered_copy_finishes_with_the_complete_payload() {
    let (_fixture, scene) = filtered_scene();
    let mut harness = Harness::default();
    assert_eq!(scene.run(&mut harness, 0), JobOutcome::Done);
    assert_eq!(scene.target(), vec![b'x'; 8 * 1024 * 1024]);
    assert_eq!(harness.progress.last().unwrap().bytes_done, 8 * 1024 * 1024);
}

#[test]
fn git_filtered_copy_fails_when_the_filter_exits_non_zero_after_bytes() {
    let (fixture, scene) = filtered_scene();
    fixture.write(".git/smudge.sh", b"head -c 65536 /dev/zero\nexit 3\n");
    let outcome = scene.run(&mut Harness::default(), 0);
    assert_eq!(refused(&outcome), FsErrorKind::SourceFilterFailed);
    assert!(scene.names().is_empty());
}
