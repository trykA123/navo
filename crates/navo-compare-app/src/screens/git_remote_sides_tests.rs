//! Tests of the remote sides in the compare run and the diff, over a local test repository.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use sidewise_core::Location;
use sidewise_engine::model::{CancelToken, Reason, RelPath, SideSource};
use sidewise_git::fixture::Fixture;
use sidewise_git_remote::at::{self, RemoteAt};
use sidewise_git_remote::{Cache, RemoteProblem, RemoteRev, RemoteUrl};
use sidewise_host::compare::handle::{CompareTarget, OpenFailure};
use tempfile::TempDir;

use super::super::git_sides::GitSides;
use super::{Fetch, remote_cache, remote_side_with, target_with};

/// A remote address that never resolves: the tests never reach the network.
fn remote_at(rev: &str) -> RemoteAt {
    RemoteAt {
        url: RemoteUrl::parse("https://test.invalid/owner/repo").expect("url"),
        rev: RemoteRev::parse(rev).expect("rev"),
        inner: "a.txt".to_owned(),
    }
}

/// The `git-remote` location of `remote_at(rev)`.
fn location(rev: &str) -> Location {
    let at = remote_at(rev);
    at::encode(&at.url, &at.rev, &at.inner).expect("location")
}

/// Copies the folder `from` into `to`, files and folders alike.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create");
    for entry in std::fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).expect("copy");
        }
    }
}

/// Stores the repository `from` in the cache folder `root` as a fetch of `at` leaves it:
/// `<key>.git` plus the local ref of the revision, which points at `tip`.
fn store(root: &Path, from: &Path, tip: &str, at: &RemoteAt) {
    let live = root.join(format!("{}.git", at.url.cache_key()));
    copy_tree(from, &live);
    let hash = blake3::hash(at.rev.as_str().as_bytes()).to_hex();
    let refs = live.join(".git/refs/sidewise");
    std::fs::create_dir_all(&refs).expect("refs folder");
    std::fs::write(refs.join(&hash.as_str()[..16]), format!("{tip}\n")).expect("ref");
}

/// A cache folder and a test repository that a fake fetch copies into it.
struct Remote {
    root: TempDir,
    cache: Arc<Cache>,
    fixture: Fixture,
    tip: String,
}

impl Remote {
    fn new() -> Self {
        let fixture = Fixture::new();
        let tip = fixture.commit("main", &[("a.txt", b"hello")]).to_string();
        let root = TempDir::new().expect("temp folder");
        let cache = Arc::new(Cache::new(root.path().to_path_buf()));
        Remote {
            root,
            cache,
            fixture,
            tip,
        }
    }

    /// A fetch that stores the repository, counts its calls and answers as `local_at` does.
    fn fetch(&self, calls: &Arc<AtomicUsize>) -> Fetch {
        let (cache, calls) = (Arc::clone(&self.cache), Arc::clone(calls));
        let (root, repo) = (
            self.root.path().to_path_buf(),
            self.fixture.path().to_path_buf(),
        );
        let tip = self.tip.clone();
        Arc::new(move |at: &RemoteAt| {
            calls.fetch_add(1, Ordering::SeqCst);
            store(&root, &repo, &tip, at);
            cache.local_at(at).ok_or(RemoteProblem::NotFound)
        })
    }
}

/// Runs the opener of a `Side` target.
fn open(target: CompareTarget) -> Result<Arc<dyn SideSource>, OpenFailure> {
    match target {
        CompareTarget::Side(opener) => opener(),
        CompareTarget::Local(_) => panic!("a remote location must not open as a local folder"),
    }
}

#[test]
fn a_run_fetches_once_and_opens_the_side() {
    let remote = Remote::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let target = target_with(
        &location("main"),
        Arc::new(GitSides::isolated()),
        remote.fetch(&calls),
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "building the target is free"
    );
    let side = open(target).expect("the side opens");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let mut names = Vec::new();
    side.list(
        &RelPath::new(sidewise_core::ProviderPath::from_text("")),
        &mut |entry| {
            names.push(entry.rel.as_path().to_text());
            std::ops::ControlFlow::Continue(())
        },
        &CancelToken::new(),
    )
    .expect("list");
    assert!(names.contains(&"a.txt".to_owned()), "{names:?}");
}

#[test]
fn the_diff_reads_what_the_run_fetched_and_never_fetches() {
    let remote = Remote::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let sides = Arc::new(GitSides::isolated());
    let local = |at: &RemoteAt| remote.cache.local_at(at);
    let before = remote_side_with(&location("main"), &sides, &local);
    assert_eq!(before.err(), Some(Reason::NotFound), "nothing fetched yet");
    let target = target_with(&location("main"), Arc::clone(&sides), remote.fetch(&calls));
    drop(open(target).expect("the side opens"));
    let (source, entry) =
        remote_side_with(&location("main"), &sides, &local).expect("the fetched file");
    assert_eq!(entry.rel.as_path().to_text(), "a.txt");
    assert!(source.stamp(&entry.rel).is_ok());
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the diff did not fetch");
}

#[test]
fn a_failed_fetch_reaches_the_tab_with_its_text() {
    let fetch: Fetch = Arc::new(|_| Err(RemoteProblem::NotFound));
    let target = target_with(&location("main"), Arc::new(GitSides::isolated()), fetch);
    assert_eq!(
        open(target).err(),
        Some(OpenFailure {
            reason: Reason::NotFound,
            text_id: Some("git-remote-not-found"),
        })
    );
}

#[test]
fn a_bad_location_is_unsupported() {
    let local = Location::local(sidewise_core::ProviderPath::from_text("/tmp")).expect("local");
    let fetch: Fetch = Arc::new(|_| Err(RemoteProblem::Broken));
    let target = target_with(&local, Arc::new(GitSides::isolated()), fetch);
    assert_eq!(open(target).err(), Some(Reason::Unsupported.into()));
}

#[test]
fn there_is_one_remote_cache_per_process() {
    let first = remote_cache().expect("a cache");
    let second = remote_cache().expect("a cache");
    assert!(std::ptr::eq(first, second));
}

#[cfg(not(windows))]
#[test]
fn the_hooks_route_a_remote_location_and_it_is_unsupported_off_windows() {
    use super::super::git_sides::{side_fn, target_for};

    let sides = Arc::new(GitSides::isolated());
    let opened = open(target_for(Arc::clone(&sides))(&location("main")));
    assert_eq!(
        opened.err().map(|failure| failure.text_id),
        Some(Some("git-remote-broken"))
    );
    let diff = side_fn(sides)(&location("main"), &Default::default());
    assert_eq!(
        diff.err(),
        Some(Reason::NotFound),
        "never fetched, so nothing to read"
    );
}
