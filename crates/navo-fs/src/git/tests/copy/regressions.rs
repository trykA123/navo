use super::*;
use crate::SourceOps;

#[test]
fn a_job_copies_the_pinned_commit_after_a_branch_moves() {
    let _guard = serial();
    let fixture = fixture().expect("Git is required for the pinned copy regression");
    fixture.init();
    fixture.write("file.txt", b"pinned\n");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("pinned");
    let sources = ["main", "HEAD"].map(|rev| {
        let git = Arc::new(GitFs::new());
        let source = at(&fixture, rev, "file.txt");
        let metadata = SourceOps::metadata(git.as_ref(), &source).unwrap();
        (git, source, metadata)
    });
    fixture.write("file.txt", b"moved!\n");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("moved");
    for (git, source, metadata) in sources {
        for workers in [0, 1, 4] {
            assert_copy(git.clone(), &source, workers, b"pinned\n");
            assert_eq!(
                SourceOps::metadata(git.as_ref(), &source).unwrap(),
                metadata
            );
        }
    }
}

#[test]
fn a_job_refuses_an_lfs_pointer_when_smudging_fails() {
    let _guard = serial();
    let fixture = fixture().expect("Git is required for the LFS copy regression");
    fixture.init();
    fixture.write(".gitattributes", b"file.txt filter=lfs -text\n");
    fixture.git_ok(&["add", ".gitattributes"]);
    let pointer = b"version https://git-lfs.github.com/spec/v1\noid sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\nsize 1024\n";
    fixture.add_blob("100644", pointer, "file.txt");
    fixture.commit("LFS pointer");
    fixture.git_ok(&["config", "filter.lfs.smudge", "git navo-missing-smudge"]);
    for required in ["false", "true"] {
        fixture.git_ok(&["config", "filter.lfs.required", required]);
        assert_filter_refused(&fixture, pointer);
    }
}

#[test]
fn a_job_refuses_raw_content_when_a_custom_filter_fails() {
    let _guard = serial();
    let fixture = fixture().expect("Git is required for the filter copy regression");
    fixture.init();
    fixture.write(".gitattributes", b"file.txt filter=custom -text\n");
    fixture.write("file.txt", b"raw\n");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("custom filter");
    fixture.git_ok(&["config", "filter.custom.smudge", "git navo-missing-smudge"]);
    assert_filter_refused(&fixture, b"raw\n");
}

fn assert_filter_refused(fixture: &Fixture, raw: &[u8]) {
    let git = Arc::new(GitFs::new());
    let source = at(fixture, "HEAD", "file.txt");
    assert_eq!(read(&git, &source), raw);
    assert!(matches!(
        git.last_problem(&source),
        Some(GitProblem::Filter(_))
    ));
    for workers in [0, 1, 4] {
        assert_refused(
            git.clone(),
            &source,
            workers,
            FsErrorKind::SourceFilterFailed,
        );
    }
}

#[test]
fn a_job_refuses_a_git_symlink_as_unsupported() {
    let _guard = serial();
    let fixture = fixture().expect("Git is required for the symlink copy regression");
    fixture.init();
    fixture.add_blob("120000", b"file.txt", "link");
    fixture.commit("symlink");
    let git = Arc::new(GitFs::new());
    let source = at(&fixture, "HEAD", "link");
    for workers in [0, 1, 4] {
        assert_refused(git.clone(), &source, workers, FsErrorKind::NotSupportedYet);
    }
}

#[test]
fn a_job_with_the_gix_setting_uses_git_for_worktree_conversion() {
    use navo_git::{EngineKind, EngineSetting, GitFound, GixGit, RepoFormat, choose};

    let _guard = serial();
    let fixture = fixture().expect("Git is required for the gix copy regression");
    fixture.init();
    fixture.write(".gitattributes", b"file.txt text eol=crlf\n");
    fixture.write("file.txt", b"a\nb\n");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("conversion");
    let format = RepoFormat::read(fixture.repo()).unwrap();
    let choice = choose(EngineSetting::Gix, &format, GitFound::version(2, 36));
    assert_eq!(choice.engine, EngineKind::Gix);
    let git = Arc::new(GitFs::with_factory(|repo| Ok(Box::new(GixGit::new(repo)?))));
    let source = at(&fixture, "HEAD", "file.txt");
    for workers in [0, 1, 4] {
        assert_copy(git.clone(), &source, workers, b"a\r\nb\r\n");
    }
}

fn assert_copy(git: Arc<GitFs>, source: &Location, workers: usize, expected: &[u8]) {
    let out = tempfile::tempdir().unwrap();
    let target = out.path().join("file.txt");
    let size = Provider::metadata(git.as_ref(), source).unwrap().size;
    let plan = conversion_plan(source.clone(), local_of(&target), size);
    assert_eq!(run(git, &plan, workers), crate::JobOutcome::Done);
    assert_eq!(fs::read(target).unwrap(), expected);
}

fn assert_refused(git: Arc<GitFs>, source: &Location, workers: usize, kind: FsErrorKind) {
    let out = tempfile::tempdir().unwrap();
    let target = out.path().join("file.txt");
    let size = Provider::metadata(git.as_ref(), source).unwrap().size;
    let plan = conversion_plan(source.clone(), local_of(&target), size);
    let outcome = run(git, &plan, workers);
    assert!(
        matches!(&outcome, crate::JobOutcome::Failed { error, .. } if error.kind == kind),
        "{outcome:?}"
    );
    assert_eq!(fs::read_dir(out.path()).unwrap().count(), 0);
}

fn run(git: Arc<GitFs>, plan: &navo_core::OperationPlan, workers: usize) -> crate::JobOutcome {
    crate::run_job_parallel_with_sources(
        (plan, &crate::Sources::new().with(git)),
        &[],
        &mut crate::JobContext {
            ops: &crate::LocalFs::new(),
            log: &mut Steps,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
        workers,
    )
}
