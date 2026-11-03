use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use navo_git::GitError;
use navo_git::cli::Cancel as GitCancel;
use navo_git::status::parse_porcelain_v2;
use navo_git::trust::Refusal;

use super::*;

struct FakeBackend {
    opened: Mutex<Vec<PathBuf>>,
    open: fn() -> Result<(), GitError>,
    status: fn() -> Result<RepoStatus, GitError>,
}

struct FakeReader(fn() -> Result<RepoStatus, GitError>);

impl Reader for FakeReader {
    fn status(&self, _: &GitCancel) -> Result<RepoStatus, GitError> {
        (self.0)()
    }
}

impl Backend for FakeBackend {
    fn open(&self, repo: &Path) -> Result<Box<dyn Reader>, GitError> {
        self.opened.lock().unwrap().push(repo.to_path_buf());
        (self.open)()?;
        Ok(Box::new(FakeReader(self.status)))
    }
}

fn clean() -> Result<RepoStatus, GitError> {
    parse_porcelain_v2(b"# branch.oid (initial)\0# branch.head main\0", false)
}

fn backend(
    open: fn() -> Result<(), GitError>,
    status: fn() -> Result<RepoStatus, GitError>,
) -> FakeBackend {
    FakeBackend {
        opened: Mutex::default(),
        open,
        status,
    }
}

fn run(backend: FakeBackend, engine: GitEngine) -> Result<RepoStatus, ScanError> {
    run_with(backend, self::backend(ok, clean), engine)
}

fn run_with(
    cli: FakeBackend,
    gix: FakeBackend,
    engine: GitEngine,
) -> Result<RepoStatus, ScanError> {
    GitSource::new(cli, gix, engine).status(Path::new("repo"), &Cancel::new())
}

fn ok() -> Result<(), GitError> {
    Ok(())
}

fn no_git() -> Result<(), GitError> {
    Err(GitError::GitNotFound)
}

fn too_old() -> Result<(), GitError> {
    Err(GitError::GitTooOld {
        version: "2.30".to_owned(),
    })
}

fn unsafe_dir() -> Result<(), GitError> {
    Err(GitError::UnsafeDirectory {
        path: PathBuf::from("repo"),
    })
}

fn blocked() -> Result<RepoStatus, GitError> {
    Err(GitError::NotTrusted {
        path: PathBuf::from("repo"),
    })
}

fn timeout() -> Result<RepoStatus, GitError> {
    Err(GitError::Timeout)
}

fn broken() -> Result<RepoStatus, GitError> {
    Err(GitError::Protocol {
        detail: "x".to_owned(),
    })
}

#[test]
fn a_found_git_gives_the_status_for_every_engine_setting() {
    for engine in [GitEngine::Auto, GitEngine::Git, GitEngine::Gix] {
        let status = run(backend(ok, clean), engine).unwrap();
        assert_eq!(status.changed, 0, "{engine:?}");
    }
}

#[test]
fn the_git_setting_without_git_gives_no_git() {
    for open in [no_git as fn() -> Result<(), GitError>, too_old] {
        assert_eq!(
            run_with(backend(open, clean), backend(ok, clean), GitEngine::Git),
            Err(ScanError::NoGit)
        );
    }
}

fn filter_refused() -> Result<RepoStatus, GitError> {
    Err(GitError::FilterRefused {
        path: PathBuf::from("repo"),
        filter: "crypt".to_owned(),
    })
}

fn program_refused() -> Result<RepoStatus, GitError> {
    Err(GitError::ProgramConfigured {
        path: PathBuf::from("repo"),
    })
}

fn unsupported() -> Result<RepoStatus, GitError> {
    Err(GitError::Unsupported {
        operation: "reftable references".to_owned(),
    })
}

type Status = fn() -> Result<RepoStatus, GitError>;

fn opened(backend: &FakeBackend) -> usize {
    backend.opened.lock().unwrap().len()
}

#[test]
fn auto_reads_with_gix_and_never_starts_git_when_gix_can() {
    let source = GitSource::new(backend(ok, clean), backend(ok, clean), GitEngine::Auto);
    source.status(Path::new("r"), &Cancel::new()).unwrap();
    assert_eq!(opened(&source.gix), 1);
    assert_eq!(opened(&source.cli), 0);
}

#[test]
fn the_git_setting_never_opens_gix_and_the_gix_setting_never_opens_the_cli() {
    let source = GitSource::new(backend(ok, clean), backend(ok, clean), GitEngine::Git);
    source.status(Path::new("r"), &Cancel::new()).unwrap();
    assert_eq!(opened(&source.gix), 0);
    let source = GitSource::new(backend(ok, clean), backend(ok, clean), GitEngine::Gix);
    source.status(Path::new("r"), &Cancel::new()).unwrap();
    assert_eq!(opened(&source.cli), 0);
}

#[test]
fn auto_falls_back_to_the_cli_when_gix_refuses_for_a_reason_git_can_handle() {
    let refusals: [Status; 4] = [filter_refused, program_refused, unsupported, unsafe_status];
    for refusal in refusals {
        let source = GitSource::new(backend(ok, clean), backend(ok, refusal), GitEngine::Auto);
        let status = source.status(Path::new("r"), &Cancel::new()).unwrap();
        assert_eq!(status.changed, 0);
        assert_eq!((opened(&source.gix), opened(&source.cli)), (1, 1));
    }
}

fn unsafe_status() -> Result<RepoStatus, GitError> {
    Err(GitError::UnsafeDirectory {
        path: PathBuf::from("repo"),
    })
}

#[test]
fn auto_remembers_the_cli_for_a_repository_but_not_for_its_neighbours() {
    let source = GitSource::new(
        backend(ok, clean),
        backend(ok, filter_refused),
        GitEngine::Auto,
    );
    for _ in 0..3 {
        source.status(Path::new("a"), &Cancel::new()).unwrap();
    }
    assert_eq!((opened(&source.gix), opened(&source.cli)), (1, 3));
    source.status(Path::new("b"), &Cancel::new()).unwrap();
    assert_eq!((opened(&source.gix), opened(&source.cli)), (2, 4));
}

#[test]
fn a_blocked_guard_does_not_fall_back_since_the_cli_would_refuse_too() {
    let source = GitSource::new(backend(ok, clean), backend(ok, blocked), GitEngine::Auto);
    let result = source.status(Path::new("r"), &Cancel::new());
    assert_eq!(result, Err(ScanError::Refused(Refusal::Guard)));
    assert_eq!(opened(&source.cli), 0);
}

#[test]
fn auto_with_a_gix_refusal_and_no_usable_git_says_no_git() {
    for open in [no_git as fn() -> Result<(), GitError>, too_old] {
        let source = GitSource::new(
            backend(open, clean),
            backend(ok, filter_refused),
            GitEngine::Auto,
        );
        assert_eq!(
            source.status(Path::new("r"), &Cancel::new()),
            Err(ScanError::NoGit)
        );
    }
}

#[test]
fn the_gix_setting_keeps_its_refusal_reasons_and_never_falls_back() {
    let cases: [(Status, Refusal); 4] = [
        (filter_refused, Refusal::Filter),
        (program_refused, Refusal::Program),
        (unsafe_status, Refusal::Ownership),
        (blocked, Refusal::Guard),
    ];
    for (status, refusal) in cases {
        let source = GitSource::new(backend(ok, clean), backend(ok, status), GitEngine::Gix);
        assert_eq!(
            source.status(Path::new("r"), &Cancel::new()),
            Err(ScanError::Refused(refusal))
        );
        assert_eq!(opened(&source.cli), 0);
    }
}

#[test]
fn an_unsafe_directory_the_cli_refuses_too_is_an_ownership_refusal() {
    assert_eq!(
        run(backend(unsafe_dir, clean), GitEngine::Git),
        Err(ScanError::Refused(Refusal::Ownership))
    );
}

#[test]
fn timeouts_and_other_failures_keep_their_kind() {
    assert_eq!(
        run_with(backend(ok, clean), backend(ok, timeout), GitEngine::Auto),
        Err(ScanError::TimedOut)
    );
    assert_eq!(
        run_with(backend(ok, clean), backend(ok, broken), GitEngine::Auto),
        Err(ScanError::Failed)
    );
}

#[test]
fn the_source_opens_the_repository_it_was_asked_about() {
    let backend = backend(ok, clean);
    let source = GitSource::new(self::backend(ok, clean), backend, GitEngine::Auto);
    source.status(Path::new("a/b"), &Cancel::new()).unwrap();
    assert_eq!(*source.gix.opened.lock().unwrap(), [PathBuf::from("a/b")]);
}

struct Waiting;

impl Reader for Waiting {
    fn status(&self, cancel: &GitCancel) -> Result<RepoStatus, GitError> {
        for _ in 0..500 {
            if cancel.is_cancelled() {
                return Err(GitError::Cancelled);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        clean()
    }
}

struct WaitingBackend;

impl Backend for WaitingBackend {
    fn open(&self, _: &Path) -> Result<Box<dyn Reader>, GitError> {
        Ok(Box::new(Waiting))
    }
}

#[test]
fn cancelling_the_scan_token_cancels_the_git_call_within_100_ms() {
    let source = GitSource::new(WaitingBackend, WaitingBackend, GitEngine::Auto);
    let token = Cancel::new();
    let started = std::time::Instant::now();
    let result = std::thread::scope(|scope| {
        let call = scope.spawn(|| source.status(Path::new("repo"), &token));
        std::thread::sleep(Duration::from_millis(30));
        token.cancel();
        call.join().unwrap()
    });
    assert_eq!(result, Err(ScanError::Cancelled));
    assert!(started.elapsed() < Duration::from_millis(400));
}

/// An empty repository whose attributes name `filter`, with that driver set in a user config.
fn filter_repo(root: &Path, filter: &str, clean: &str) -> PathBuf {
    let repo = root.join("repo");
    for dir in [".git/objects", ".git/refs/heads"] {
        std::fs::create_dir_all(repo.join(dir)).unwrap();
    }
    std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    std::fs::write(
        repo.join(".git/config"),
        "[core]\n\trepositoryformatversion = 0\n",
    )
    .unwrap();
    std::fs::write(
        repo.join(".gitattributes"),
        format!("*.bin filter={filter} -text\n"),
    )
    .unwrap();
    let global = root.join("global.gitconfig");
    let text = format!("[filter \"{filter}\"]\n\tclean = {clean}\n\tsmudge = {clean}\n");
    std::fs::write(&global, text).unwrap();
    // SAFETY: nextest runs each test in its own process and this test spawns no thread first.
    unsafe {
        std::env::set_var("GIT_CONFIG_GLOBAL", &global);
        std::env::set_var("GIT_CONFIG_NOSYSTEM", "1");
    }
    repo
}

#[test]
fn auto_reads_an_lfs_repository_with_gix_and_starts_no_git() {
    let root = tempfile::tempdir().unwrap();
    let repo = filter_repo(root.path(), "lfs", "git-lfs clean -- %f");
    let source = GitSource::new(backend(ok, clean), GixBackend, GitEngine::Auto);
    let status = source.status(&repo, &Cancel::new()).unwrap();
    assert_eq!(status.changed, 0);
    assert_eq!(opened(&source.cli), 0);
}

#[test]
fn auto_reads_a_custom_filter_repository_with_the_cli() {
    let root = tempfile::tempdir().unwrap();
    let repo = filter_repo(root.path(), "crypt", "crypt %f");
    let source = GitSource::new(backend(ok, clean), GixBackend, GitEngine::Auto);
    source.status(&repo, &Cancel::new()).unwrap();
    assert_eq!(opened(&source.cli), 1);
    let gix_only = GitSource::new(backend(ok, clean), GixBackend, GitEngine::Gix);
    assert_eq!(
        gix_only.status(&repo, &Cancel::new()),
        Err(ScanError::Refused(Refusal::Filter))
    );
}
