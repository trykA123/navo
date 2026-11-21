//! Live proof of the remote fetch (RG9, NC226): WinHTTP and gix against public GitHub, on the VM.
//! Every test is ignored ("live network"); run with `--ignored --nocapture --test-threads=1`.
#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test: a failed setup step should abort the test"
)]
#![allow(
    clippy::print_stdout,
    reason = "each case prints its RG9 result line for the VM log"
)]

use std::path::PathBuf;
use std::time::Instant;

use sidewise_core::ProviderPath;
use sidewise_engine::model::{RelPath, SideSource};
use sidewise_git::location::GitAt;
use sidewise_git::side::GitTreeSide;
use sidewise_git_remote::{Cache, Limits, RemoteAt, RemoteProblem, RemoteRev, RemoteUrl, WinHttp};

/// The public repository every case reads.
const REPO: &str = "https://github.com/trykA123/sidewise";
/// The folder that holds every cache root of this file; removed by the VM script at the end.
const TEST_ROOT: &str = r"C:\navo-test\rg9";

/// A cache root `TEST_ROOT\<name>`; `fresh` removes what an earlier run left there.
fn root(name: &str, fresh: bool) -> PathBuf {
    let dir = PathBuf::from(TEST_ROOT).join(name);
    if fresh && dir.exists() {
        std::fs::remove_dir_all(&dir).unwrap();
    }
    dir
}

/// The remote side `url` at `rev` with no inner path; panics on an address the parser refuses.
fn at(url: &str, rev: &str) -> RemoteAt {
    RemoteAt {
        url: RemoteUrl::parse(url).unwrap(),
        rev: RemoteRev::parse(rev).unwrap(),
        inner: String::new(),
    }
}

/// Fetches `at` into the cache at `dir` through WinHTTP with the product limits and prints
/// the `RG9 <case>: <result> ms=<elapsed>` line; returns the fetch result.
fn fetch(case: &str, dir: PathBuf, at: &RemoteAt) -> Result<GitAt, RemoteProblem> {
    let cache = Cache::new(dir);
    let start = Instant::now();
    let got = cache.fetch_with(at, WinHttp::new(), Limits::PRODUCT);
    let ms = start.elapsed().as_millis();
    let shown = match &got {
        Ok(_) => "Ok".to_owned(),
        Err(problem) => format!("{problem:?}"),
    };
    println!("RG9 {case}: {shown} ms={ms}");
    got
}

/// Opens the fetched side as P2a does and asserts `README.md` is a file at the tree root.
fn assert_readme(git_at: &GitAt) {
    let side = GitTreeSide::open(&git_at.repo, &git_at.rev, git_at.form).unwrap();
    let readme = RelPath::new(ProviderPath::from_bytes(b"README.md"));
    let stamp = side.stamp(&readme).unwrap();
    assert!(stamp.size > 0, "README.md is empty");
}

#[test]
#[ignore = "live network"]
fn case1_main_branch() {
    let got = fetch("1 main", root("main", true), &at(REPO, "main")).unwrap();
    assert_readme(&got);
}

#[test]
#[ignore = "live network"]
fn case2_tag() {
    let got = fetch(
        "2 tag v0.1.0-alpha.2",
        root("tag", true),
        &at(REPO, "v0.1.0-alpha.2"),
    );
    assert_readme(&got.unwrap());
}

#[test]
#[ignore = "live network"]
fn case3_pull_head() {
    let got = fetch(
        "3 refs/pull/1/head",
        root("pull", true),
        &at(REPO, "refs/pull/1/head"),
    );
    assert_readme(&got.unwrap());
}

#[test]
#[ignore = "live network"]
fn case4_missing_repo_is_not_found() {
    let missing = "https://github.com/trykA123/no-such-repo-rg9-20261118";
    let got = fetch(
        "4 missing repo",
        root("missing", true),
        &at(missing, "main"),
    );
    assert_eq!(got, Err(RemoteProblem::NotFound));
}

#[test]
#[ignore = "live network"]
fn case5_unknown_host_is_unreachable() {
    let got = fetch(
        "5 unknown host",
        root("dns", true),
        &at("https://nonexistent.invalid/x/y", "main"),
    );
    assert_eq!(got, Err(RemoteProblem::Unreachable));
}

#[test]
#[ignore = "live network"]
fn case6_missing_branch_is_no_such_revision() {
    let got = fetch(
        "6 missing branch",
        root("nobranch", true),
        &at(REPO, "no-such-branch-rg9"),
    );
    assert_eq!(got, Err(RemoteProblem::NoSuchRevision));
}

#[test]
#[ignore = "live network"]
fn case7_refetch_keeps_one_repository() {
    let main = at(REPO, "main");
    // Reuses case 1's root; a lone run fetches it first so the second fetch is a refetch.
    let dir = root("main", false);
    if !dir.exists() {
        fetch("7 first fetch", dir.clone(), &main).unwrap();
    }
    let got = fetch("7 refetch main", dir.clone(), &main).unwrap();
    assert_readme(&got);
    let repos: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".git"))
        .collect();
    assert_eq!(repos, vec![format!("{}.git", main.url.cache_key())]);
}
