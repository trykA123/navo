//! Shared test support: the CGI `Http` backend and the fixture repositories (built once per run).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test support: a failed setup step should abort the test"
)]

mod backend;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

pub use backend::CgiHttp;

/// The fixture: a folder holding bare repositories, and the commit `main` points at.
pub struct Fixture {
    root: PathBuf,
    main_oid: String,
}

impl Fixture {
    /// The folder that holds `repo.git`.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The hex object id of `refs/heads/main` in `repo.git`.
    pub fn main_oid(&self) -> &str {
        &self.main_oid
    }
}

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

/// Deterministic incompressible bytes (xorshift64), standing in for a random blob.
fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut bytes = Vec::with_capacity(len);
    while bytes.len() < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        bytes.extend_from_slice(&state.to_le_bytes());
    }
    bytes.truncate(len);
    bytes
}

/// Removes fixture folders left by earlier runs whose process no longer exists.
fn remove_stale(parent: &Path) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(pid) = name
            .strip_prefix("rg4-")
            .and_then(|p| p.parse::<u32>().ok())
        else {
            continue;
        };
        if !Path::new(&format!("/proc/{pid}")).exists() {
            std::fs::remove_dir_all(entry.path()).ok();
        }
    }
}

/// Builds `repo.git`: branch `main`, tag `v1`, `refs/pull/1/head`, a 2 MiB random blob, an 80 MiB zero blob.
fn build() -> Fixture {
    let parent = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    remove_stale(&parent);
    let root = parent.join(format!("rg4-{}", std::process::id()));
    let work = root.join("work");
    std::fs::create_dir_all(&work).unwrap();
    git(&work, &["init", "-q", "-b", "main"]);
    std::fs::write(work.join("readme.txt"), "fixture\n").unwrap();
    std::fs::write(work.join("random.bin"), noise(2 << 20)).unwrap();
    std::fs::write(work.join("zero.bin"), vec![0_u8; 80 << 20]).unwrap();
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "fixture"]);
    git(&work, &["clone", "-q", "--bare", ".", "../repo.git"]);
    let bare = root.join("repo.git");
    git(&bare, &["tag", "v1", "main"]);
    git(&bare, &["update-ref", "refs/pull/1/head", "main"]);
    let main_oid = git(&bare, &["rev-parse", "refs/heads/main"]);
    std::fs::remove_dir_all(&work).unwrap();
    Fixture { root, main_oid }
}

/// The fixture, built on first use and shared by every test in the run.
pub fn fixture() -> &'static Fixture {
    static FIXTURE: OnceLock<Fixture> = OnceLock::new();
    FIXTURE.get_or_init(build)
}

/// A protocol v2 `fetch` request for `oid`, as pkt-lines.
pub fn fetch_request(oid: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for line in ["command=fetch\n", "agent=test\n"] {
        out.extend_from_slice(format!("{:04x}{line}", line.len() + 4).as_bytes());
    }
    out.extend_from_slice(b"0001");
    for line in [format!("want {oid}\n"), "done\n".to_owned()] {
        out.extend_from_slice(format!("{:04x}{line}", line.len() + 4).as_bytes());
    }
    out.extend_from_slice(b"0000");
    out
}
