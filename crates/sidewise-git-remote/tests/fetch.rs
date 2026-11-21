//! Tests for `fetch_with`: one shallow ref fetch into a cache repository, over the CGI backend.
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

use std::io::{self, BufRead, Read};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use gix_transport::client::AuthenticationRequired;
use gix_transport::client::blocking_io::http::{GetResponse, Http, PostBodyDataKind, PostResponse};

use sidewise_git_remote::{Limits, RemoteProblem, RemoteRev, RemoteUrl, fetch_with};
use support::{CgiHttp, fixture};
use tempfile::TempDir;

/// Runs `git` with an isolated config in `dir` and returns trimmed stdout; panics on failure.
fn git(dir: &Path, args: &[&str]) -> String {
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
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A fresh root holding `small.git`: two commits on `main`, tag `v1` and `refs/pull/1/head`
/// on the first one.
fn small_root() -> TempDir {
    let root = TempDir::new().unwrap();
    let work = root.path().join("work");
    std::fs::create_dir(&work).unwrap();
    git(&work, &["init", "-q", "-b", "main"]);
    for text in ["one", "two"] {
        std::fs::write(work.join("a.txt"), text).unwrap();
        git(&work, &["add", "a.txt"]);
        git(&work, &["commit", "-q", "-m", text]);
        if text == "one" {
            git(&work, &["tag", "v1"]);
        }
    }
    git(&work, &["clone", "-q", "--bare", ".", "../small.git"]);
    let bare = root.path().join("small.git");
    git(&bare, &["update-ref", "refs/pull/1/head", "v1"]);
    std::fs::remove_dir_all(&work).unwrap();
    root
}

/// Limits large enough for every fixture, with a 60 s deadline.
const ROOMY: Limits = Limits {
    max_bytes: 256 << 20,
    deadline: Duration::from_secs(60),
};

/// Fetches `(path, rev)` of `https://test.invalid/<path>` served from `root` into `cache`.
fn fetch(
    root: &Path,
    (path, rev): (&str, &str),
    cache: &Path,
    limits: Limits,
) -> Result<String, RemoteProblem> {
    let url = RemoteUrl::parse(&format!("https://test.invalid/{path}")).unwrap();
    let rev = RemoteRev::parse(rev).unwrap();
    fetch_with(cache, &url, &rev, CgiHttp::new(root), limits)
}

/// The object id `name` points to in the repository at `dir`.
fn oid(dir: &Path, name: &str) -> String {
    git(dir, &["rev-parse", name])
}

/// Fetches `rev` of `small.git` into a fresh cache and checks the local ref against `expect`.
fn fetches_to(rev: &str, expect: &str) {
    let root = small_root();
    let cache = TempDir::new().unwrap();
    let dir = cache.path().join("c.git");
    let local = fetch(root.path(), ("small.git", rev), &dir, ROOMY).unwrap();
    assert!(local.starts_with("refs/sidewise/"), "{local}");
    assert_eq!(local.len(), "refs/sidewise/".len() + 16, "{local}");
    let bare = root.path().join("small.git");
    assert_eq!(oid(&dir, &local), oid(&bare, expect));
    assert!(dir.join("shallow").is_file(), "the fetch is shallow");
}

#[test]
fn fetches_a_branch() {
    fetches_to("main", "refs/heads/main");
}

#[test]
fn fetches_a_tag() {
    fetches_to("v1", "refs/tags/v1");
}

#[test]
fn fetches_a_pull_ref() {
    fetches_to("refs/pull/1/head", "refs/pull/1/head");
}

#[test]
fn fetches_head() {
    fetches_to("", "HEAD");
    fetches_to("HEAD", "HEAD");
}

#[test]
fn refetch_moves_the_ref() {
    let root = small_root();
    let bare = root.path().join("small.git");
    let cache = TempDir::new().unwrap();
    let dir = cache.path().join("c.git");
    let first = fetch(root.path(), ("small.git", "main"), &dir, ROOMY).unwrap();
    let old = oid(&dir, &first);
    git(&bare, &["update-ref", "refs/heads/main", "v1"]);
    let second = fetch(root.path(), ("small.git", "main"), &dir, ROOMY).unwrap();
    assert_eq!(first, second);
    assert_ne!(oid(&dir, &second), old);
    assert_eq!(oid(&dir, &second), oid(&bare, "v1"));
}

/// Wraps `CgiHttp` and runs `act` just before its second GET, the handshake of connection 2:
/// the server's refs change between listing and fetching.
struct Racy {
    inner: CgiHttp,
    gets: usize,
    act: Box<dyn FnMut()>,
}

impl Http for Racy {
    type Headers = <CgiHttp as Http>::Headers;
    type ResponseBody = <CgiHttp as Http>::ResponseBody;
    type PostBody = <CgiHttp as Http>::PostBody;

    fn get(
        &mut self,
        url: &str,
        base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> gix::Result<GetResponse<Self::Headers, Self::ResponseBody>> {
        self.gets += 1;
        if self.gets == 2 {
            (self.act)();
        }
        self.inner.get(url, base_url, headers)
    }

    fn post(
        &mut self,
        url: &str,
        base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
        body: PostBodyDataKind,
    ) -> gix::Result<PostResponse<Self::Headers, Self::ResponseBody, Self::PostBody>> {
        self.inner.post(url, base_url, headers, body)
    }

    fn configure(&mut self, config: &dyn std::any::Any) -> gix::Result {
        self.inner.configure(config)
    }
}

/// Fetches `main` of a fresh `small.git` while `git <args>` runs in it between the connections.
fn raced(args: &'static [&'static str]) -> Result<String, RemoteProblem> {
    let root = small_root();
    let bare = root.path().join("small.git");
    let cache = TempDir::new().unwrap();
    let http = Racy {
        inner: CgiHttp::new(root.path()),
        gets: 0,
        act: Box::new(move || {
            git(&bare, args);
        }),
    };
    let url = RemoteUrl::parse("https://test.invalid/small.git").unwrap();
    let rev = RemoteRev::parse("main").unwrap();
    fetch_with(&cache.path().join("c.git"), &url, &rev, http, ROOMY)
}

#[test]
fn ref_deleted_between_connections_is_no_such_revision() {
    let got = raced(&["update-ref", "-d", "refs/heads/main"]);
    assert_eq!(got, Err(RemoteProblem::NoSuchRevision));
}

#[test]
fn ref_moved_between_connections_is_no_such_revision() {
    let got = raced(&["update-ref", "refs/heads/main", "v1"]);
    assert_eq!(got, Err(RemoteProblem::NoSuchRevision));
}

/// Fetches `path` and `rev` from the small root into a fresh cache with `limits`; the error.
fn small_problem(path: &str, rev: &str, limits: Limits) -> RemoteProblem {
    let root = small_root();
    let cache = TempDir::new().unwrap();
    fetch(
        root.path(),
        (path, rev),
        &cache.path().join("c.git"),
        limits,
    )
    .unwrap_err()
}

#[test]
fn missing_rev_is_no_such_revision() {
    let problem = small_problem("small.git", "nope", ROOMY);
    assert_eq!(problem, RemoteProblem::NoSuchRevision);
}

#[test]
fn moved_is_redirected() {
    let problem = small_problem("moved/small.git", "main", ROOMY);
    assert_eq!(problem, RemoteProblem::Redirected);
}

#[test]
fn missing_repo_is_not_found() {
    let problem = small_problem("missing.git", "main", ROOMY);
    assert_eq!(problem, RemoteProblem::NotFound);
}

#[test]
fn truncated_pack_is_broken() {
    let problem = small_problem("truncated/small.git", "main", ROOMY);
    assert_eq!(problem, RemoteProblem::Broken);
}

#[test]
fn zero_deadline_is_timed_out() {
    let limits = Limits {
        deadline: Duration::ZERO,
        ..ROOMY
    };
    assert_eq!(
        small_problem("small.git", "main", limits),
        RemoteProblem::TimedOut
    );
}

/// Fetches `main` of the shared big fixture (2 MiB random, 80 MiB zero blob) with `limits`.
fn big_problem(limits: Limits) -> RemoteProblem {
    let cache = TempDir::new().unwrap();
    let dir = cache.path().join("c.git");
    fetch(fixture().root(), ("repo.git", "main"), &dir, limits).unwrap_err()
}

#[test]
fn byte_cap_is_too_large() {
    let limits = Limits {
        max_bytes: 1 << 20,
        ..ROOMY
    };
    assert_eq!(big_problem(limits), RemoteProblem::TooLarge);
}

#[test]
fn huge_object_is_object_too_large() {
    assert_eq!(big_problem(ROOMY), RemoteProblem::ObjectTooLarge);
}

/// A header reader that fails with a 401, as curl and reqwest report it: while headers are read.
struct Unauthorized;

impl Read for Unauthorized {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        let auth = AuthenticationRequired::default();
        Err(io::Error::new(io::ErrorKind::PermissionDenied, auth))
    }
}

impl BufRead for Unauthorized {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.read(&mut []).map(|_| &[][..])
    }

    fn consume(&mut self, _: usize) {}
}

/// Any reader of a response, so `Late401` can mix CGI readers with `Unauthorized`.
type AnyRead = Box<dyn BufRead + Unpin>;

/// Wraps `CgiHttp`, but answers every GET with the 401 in its header reader. gix starts the
/// credential helpers only for a 401 seen there, so this is what the marker test needs.
struct Late401(CgiHttp);

impl Http for Late401 {
    type Headers = AnyRead;
    type ResponseBody = AnyRead;
    type PostBody = <CgiHttp as Http>::PostBody;

    fn get(
        &mut self,
        _: &str,
        _: &str,
        _: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> gix::Result<GetResponse<AnyRead, AnyRead>> {
        let body: AnyRead = Box::new(io::empty());
        Ok(GetResponse {
            headers: Box::new(Unauthorized),
            body,
        })
    }

    fn post(
        &mut self,
        url: &str,
        base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
        body: PostBodyDataKind,
    ) -> gix::Result<PostResponse<AnyRead, AnyRead, Self::PostBody>> {
        let response = self.0.post(url, base_url, headers, body)?;
        Ok(PostResponse {
            post_body: response.post_body,
            headers: Box::new(response.headers),
            body: Box::new(response.body),
        })
    }

    fn configure(&mut self, config: &dyn std::any::Any) -> gix::Result {
        self.0.configure(config)
    }
}

/// Makes a cache repository whose own config names a helper that writes `marker`.
fn cache_with_helper(cache: &Path, marker: &Path) -> std::path::PathBuf {
    let helper = cache.join("helper.sh");
    let script = format!("#!/bin/sh\ntouch '{}'\n", marker.display());
    std::fs::write(&helper, script).unwrap();
    let mut perms = std::fs::metadata(&helper).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(&helper, perms).unwrap();
    git(cache, &["init", "-q", "--bare", "c.git"]);
    let dir = cache.join("c.git");
    git(
        &dir,
        &["config", "credential.helper", helper.to_str().unwrap()],
    );
    dir
}

#[test]
fn private_never_runs_a_credential_helper() {
    let root = small_root();
    let cache = TempDir::new().unwrap();
    let marker = cache.path().join("marker");
    let dir = cache_with_helper(cache.path(), &marker);
    let url = RemoteUrl::parse("https://test.invalid/private/small.git").unwrap();
    let rev = RemoteRev::parse("main").unwrap();
    let http = Late401(CgiHttp::new(root.path()));
    let problem = fetch_with(&dir, &url, &rev, http, ROOMY).unwrap_err();
    assert!(!marker.exists(), "a credential helper ran");
    assert_eq!(problem, RemoteProblem::NotFound);
}
