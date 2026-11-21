//! Tests for `Cache`: per-key lock, staged create, trash-and-recreate, `.used` and `local_at`.
// Runs `git http-backend` as a CGI, which only the Linux test host provides (NC226).
#![cfg(not(windows))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test: a failed setup step should abort the test"
)]

#[allow(
    dead_code,
    reason = "this test file uses only part of the shared support"
)]
mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use gix_transport::client::blocking_io::http::{GetResponse, Http, PostBodyDataKind, PostResponse};
use sidewise_git::location::Form;
use sidewise_git_remote::{Cache, Limits, RemoteAt, RemoteProblem, RemoteRev, RemoteUrl};
use support::CgiHttp;
use tempfile::TempDir;

/// Limits large enough for the fixture, with a 60 s deadline.
const ROOMY: Limits = Limits {
    max_bytes: 64 << 20,
    deadline: Duration::from_secs(60),
};

/// Runs `git` with an isolated config in `dir`; returns trimmed stdout, or `None` on failure.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "admin")
        .env("GIT_AUTHOR_EMAIL", "admin@example.invalid")
        .env("GIT_COMMITTER_NAME", "admin")
        .env("GIT_COMMITTER_EMAIL", "admin@example.invalid")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    out.status.success().then_some(text)
}

/// A served root holding `small.git` with one commit on `main`.
fn served() -> TempDir {
    let root = TempDir::new().unwrap();
    let work = root.path().join("work");
    std::fs::create_dir(&work).unwrap();
    git(&work, &["init", "-q", "-b", "main"]).unwrap();
    std::fs::write(work.join("a.txt"), "one").unwrap();
    git(&work, &["add", "a.txt"]).unwrap();
    git(&work, &["commit", "-q", "-m", "one"]).unwrap();
    git(&work, &["clone", "-q", "--bare", ".", "../small.git"]).unwrap();
    std::fs::remove_dir_all(&work).unwrap();
    root
}

/// The remote location of `rev` in `https://test.invalid/<path>`.
fn at(path: &str, rev: &str) -> RemoteAt {
    RemoteAt {
        url: RemoteUrl::parse(&format!("https://test.invalid/{path}")).unwrap(),
        rev: RemoteRev::parse(rev).unwrap(),
        inner: "dir/a.txt".to_owned(),
    }
}

/// The sorted names directly under `dir` (empty when it is missing).
fn names(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            let names = entries.map(|e| e.unwrap().file_name().to_string_lossy().into_owned());
            names.collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// True when `name` is `<key>.<kind>-<16 lower-case hex>`.
fn is_named(name: &str, key: &str, kind: &str) -> bool {
    name.strip_prefix(&format!("{key}.{kind}-"))
        .is_some_and(|hex| {
            hex.len() == 16 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
}

/// A hook run before each request a `Probe` makes.
type Hook = Arc<dyn Fn() + Send + Sync>;

/// An `Http` backend that runs a hook, then serves the request through `CgiHttp`.
struct Probe {
    inner: CgiHttp,
    hook: Hook,
}

impl Http for Probe {
    type Headers = <CgiHttp as Http>::Headers;
    type ResponseBody = <CgiHttp as Http>::ResponseBody;
    type PostBody = <CgiHttp as Http>::PostBody;

    fn get(
        &mut self,
        url: &str,
        base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> gix::error::Result<GetResponse<Self::Headers, Self::ResponseBody>> {
        (self.hook)();
        self.inner.get(url, base_url, headers)
    }

    fn post(
        &mut self,
        url: &str,
        base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
        body: PostBodyDataKind,
    ) -> gix::error::Result<PostResponse<Self::Headers, Self::ResponseBody, Self::PostBody>> {
        (self.hook)();
        self.inner.post(url, base_url, headers, body)
    }

    fn configure(&mut self, config: &dyn std::any::Any) -> gix::error::Result {
        self.inner.configure(config)
    }
}

/// Fetches `rev` of `small.git` served from `served` into `cache`.
fn fetch(
    cache: &Cache,
    served: &Path,
    rev: &str,
) -> Result<sidewise_git::location::GitAt, RemoteProblem> {
    cache.fetch_with(&at("small.git", rev), CgiHttp::new(served), ROOMY)
}

/// The cache key of the test address.
fn key() -> String {
    at("small.git", "main").url.cache_key()
}

#[test]
fn two_threads_on_one_key_fetch_one_after_the_other() {
    let served = served();
    let root = TempDir::new().unwrap();
    let cache = Arc::new(Cache::new(root.path().to_path_buf()));
    let events = Arc::new(Mutex::new(Vec::new()));
    let threads: Vec<_> = [1_u8, 2]
        .into_iter()
        .map(|tag| {
            let (cache, events, dir) = (cache.clone(), events.clone(), served.path().to_path_buf());
            std::thread::spawn(move || {
                let hook: Hook = Arc::new(move || {
                    events.lock().unwrap().push(tag);
                    // Widens the window in which an unlocked second fetch would interleave.
                    std::thread::sleep(Duration::from_millis(30));
                });
                let probe = Probe {
                    inner: CgiHttp::new(&dir),
                    hook,
                };
                cache.fetch_with(&at("small.git", "main"), probe, ROOMY)
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap().unwrap();
    }
    let events = events.lock().unwrap();
    let switches = events.windows(2).filter(|w| w[0] != w[1]).count();
    assert!(events.len() >= 4, "{events:?}");
    assert_eq!(switches, 1, "requests interleaved: {events:?}");
}

#[test]
fn first_fetch_stages_then_renames() {
    let served = served();
    let root = TempDir::new().unwrap();
    let cache = Cache::new(root.path().to_path_buf());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let (dir, record) = (root.path().to_path_buf(), seen.clone());
    let hook: Hook = Arc::new(move || record.lock().unwrap().push(names(&dir)));
    let probe = Probe {
        inner: CgiHttp::new(served.path()),
        hook,
    };
    let got = cache
        .fetch_with(&at("small.git", "main"), probe, ROOMY)
        .unwrap();
    let key = key();
    let during = seen.lock().unwrap()[0].clone();
    assert!(
        during.iter().any(|n| is_named(n, &key, "tmp")),
        "{during:?}"
    );
    assert!(!during.contains(&format!("{key}.git")), "{during:?}");
    let live = root.path().join(format!("{key}.git"));
    let after = names(root.path());
    assert!(after.iter().all(|n| !n.contains(".tmp-")), "{after:?}");
    assert_eq!(got.repo, live);
    assert_eq!((got.form, got.inner.as_str()), (Form::Stored, "dir/a.txt"));
    let main = git(&served.path().join("small.git"), &["rev-parse", "main"]);
    assert_eq!(git(&live, &["rev-parse", &got.rev]), main);
}

#[test]
fn failed_first_fetch_leaves_no_repository_and_no_staging() {
    let root = TempDir::new().unwrap();
    let empty = TempDir::new().unwrap();
    let cache = Cache::new(root.path().to_path_buf());
    let result = fetch(&cache, empty.path(), "main");
    assert_eq!(result, Err(RemoteProblem::NotFound));
    assert_eq!(names(root.path()), [format!("{}.lock", key())]);
}

#[test]
fn leftover_staging_of_the_key_is_removed_and_other_names_are_kept() {
    let served = served();
    let root = TempDir::new().unwrap();
    let key = key();
    let other = "0123456789abcdef0123456789abcdef.tmp-0123456789abcdef";
    for name in [format!("{key}.tmp-00000000deadbeef"), other.to_owned()] {
        std::fs::create_dir(root.path().join(name)).unwrap();
    }
    std::fs::write(root.path().join("notes.txt"), "keep").unwrap();
    // Another window holds the other key, so eviction must leave its staging alone.
    let other_lock = root.path().join("0123456789abcdef0123456789abcdef.lock");
    let held = std::fs::File::create(&other_lock).unwrap();
    held.lock().unwrap();
    fetch(
        &Cache::new(root.path().to_path_buf()),
        served.path(),
        "main",
    )
    .unwrap();
    let after = names(root.path());
    let expect = [
        format!("{key}.git"),
        format!("{key}.lock"),
        format!("{key}.used"),
    ];
    let mut expect: Vec<String> = expect
        .into_iter()
        .chain([other.to_owned(), "notes.txt".to_owned()])
        .chain(["0123456789abcdef0123456789abcdef.lock".to_owned()])
        .collect();
    expect.sort();
    assert_eq!(after, expect);
}

#[test]
fn repository_that_will_not_open_is_moved_to_trash_and_recreated() {
    let served = served();
    let root = TempDir::new().unwrap();
    let key = key();
    let live = root.path().join(format!("{key}.git"));
    let stuck = live.join("stuck");
    std::fs::create_dir_all(&stuck).unwrap();
    std::fs::write(live.join("HEAD"), "not a repository").unwrap();
    std::fs::write(stuck.join("marker"), "old").unwrap();
    // A folder the test user cannot empty makes `remove_dir_all` fail after the rename.
    std::fs::set_permissions(&stuck, std::fs::Permissions::from_mode(0o500)).unwrap();
    let got = fetch(
        &Cache::new(root.path().to_path_buf()),
        served.path(),
        "main",
    );
    let trash: Vec<PathBuf> = names(root.path())
        .into_iter()
        .filter(|n| is_named(n, &key, "trash"))
        .map(|n| root.path().join(n))
        .collect();
    for dir in &trash {
        std::fs::set_permissions(dir.join("stuck"), std::fs::Permissions::from_mode(0o700))
            .unwrap();
    }
    assert_eq!(got.unwrap().repo, live);
    assert!(
        git(&live, &["rev-parse", "HEAD"]).is_some(),
        "recreated repository opens"
    );
    assert!(!live.join("stuck").exists());
    assert_eq!(trash.len(), 1, "{:?}", names(root.path()));
    assert!(trash[0].join("stuck/marker").is_file());
}

#[test]
fn live_name_that_is_a_symlink_is_replaced_and_its_target_untouched() {
    let outside = served();
    let served = served();
    let root = TempDir::new().unwrap();
    let target = outside.path().join("small.git");
    let refs_before = git(&target, &["for-each-ref"]);
    let live = root.path().join(format!("{}.git", key()));
    std::os::unix::fs::symlink(&target, &live).unwrap();
    fetch(
        &Cache::new(root.path().to_path_buf()),
        served.path(),
        "main",
    )
    .unwrap();
    assert!(!live.symlink_metadata().unwrap().file_type().is_symlink());
    assert_eq!(git(&target, &["for-each-ref"]), refs_before);
}

#[test]
fn success_moves_the_used_mtime() {
    let served = served();
    let root = TempDir::new().unwrap();
    let cache = Cache::new(root.path().to_path_buf());
    fetch(&cache, served.path(), "main").unwrap();
    let used = root.path().join(format!("{}.used", key()));
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    std::fs::File::options()
        .write(true)
        .open(&used)
        .unwrap()
        .set_modified(old)
        .unwrap();
    fetch(&cache, served.path(), "main").unwrap();
    let now = std::fs::metadata(&used).unwrap().modified().unwrap();
    assert!(now > old + Duration::from_secs(86_400), "{now:?}");
}

#[test]
fn local_at_never_touches_the_network() {
    let served = served();
    let root = TempDir::new().unwrap();
    let cache = Cache::new(root.path().join("cache"));
    assert_eq!(cache.local_at(&at("small.git", "main")), None);
    assert!(names(root.path()).is_empty(), "local_at created nothing");
    let fetched = fetch(&cache, served.path(), "main").unwrap();
    // With the served repository gone, any network use would fail.
    std::fs::remove_dir_all(served.path().join("small.git")).unwrap();
    let listed = names(&root.path().join("cache"));
    assert_eq!(cache.local_at(&at("small.git", "main")), Some(fetched));
    assert_eq!(cache.local_at(&at("small.git", "v9")), None);
    assert_eq!(cache.local_at(&at("other.git", "main")), None);
    assert_eq!(names(&root.path().join("cache")), listed);
}

#[test]
fn local_at_marks_the_repository_as_recently_used() {
    let served = served();
    let root = TempDir::new().unwrap();
    let cache = Cache::new(root.path().to_path_buf());
    fetch(&cache, served.path(), "main").unwrap();
    let used = root.path().join(format!("{}.used", key()));
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    std::fs::File::options()
        .write(true)
        .open(&used)
        .unwrap()
        .set_modified(old)
        .unwrap();
    assert_eq!(cache.local_at(&at("other.git", "main")), None);
    let still = std::fs::metadata(&used).unwrap().modified().unwrap();
    assert_eq!(still, old, "a miss must not mark the repository");
    assert!(cache.local_at(&at("small.git", "main")).is_some());
    let now = std::fs::metadata(&used).unwrap().modified().unwrap();
    assert!(now > old + Duration::from_secs(86_400), "{now:?}");
}
