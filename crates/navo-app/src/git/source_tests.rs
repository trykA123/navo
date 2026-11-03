use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use navo_git::GitError;
use navo_git::cli::Cancel as GitCancel;
use navo_git::status::parse_porcelain_v2;

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

#[test]
fn auto_without_git_or_with_an_old_git_reads_with_gix() {
    for open in [no_git as fn() -> Result<(), GitError>, too_old] {
        let gix = backend(ok, clean);
        let source = GitSource::new(backend(open, clean), gix, GitEngine::Auto);
        assert_eq!(
            source
                .status(Path::new("r"), &Cancel::new())
                .unwrap()
                .changed,
            0
        );
        assert_eq!(source.gix.opened.lock().unwrap().len(), 1);
    }
}

#[test]
fn auto_with_git_never_opens_gix_and_gix_never_opens_the_cli() {
    let source = GitSource::new(backend(ok, clean), backend(ok, clean), GitEngine::Auto);
    source.status(Path::new("r"), &Cancel::new()).unwrap();
    assert!(source.gix.opened.lock().unwrap().is_empty());
    let source = GitSource::new(backend(ok, clean), backend(ok, clean), GitEngine::Gix);
    source.status(Path::new("r"), &Cancel::new()).unwrap();
    assert!(source.cli.opened.lock().unwrap().is_empty());
}

#[test]
fn a_gix_refusal_without_git_is_not_trusted() {
    assert_eq!(
        run_with(
            backend(no_git, clean),
            backend(ok, blocked),
            GitEngine::Auto
        ),
        Err(ScanError::NotTrusted)
    );
}

#[test]
fn an_unsafe_directory_and_a_blocked_guard_are_not_trusted() {
    assert_eq!(
        run(backend(unsafe_dir, clean), GitEngine::Auto),
        Err(ScanError::NotTrusted)
    );
    assert_eq!(
        run(backend(ok, blocked), GitEngine::Auto),
        Err(ScanError::NotTrusted)
    );
}

#[test]
fn timeouts_and_other_failures_keep_their_kind() {
    assert_eq!(
        run(backend(ok, timeout), GitEngine::Auto),
        Err(ScanError::TimedOut)
    );
    assert_eq!(
        run(backend(ok, broken), GitEngine::Auto),
        Err(ScanError::Failed)
    );
}

#[test]
fn the_source_opens_the_repository_it_was_asked_about() {
    let backend = backend(ok, clean);
    let source = GitSource::new(backend, self::backend(ok, clean), GitEngine::Auto);
    source.status(Path::new("a/b"), &Cancel::new()).unwrap();
    assert_eq!(*source.cli.opened.lock().unwrap(), [PathBuf::from("a/b")]);
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
