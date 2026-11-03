use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use navo_git::trust::Guard;

use super::fixture::Fixture;

static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct GitDir {
    root: PathBuf,
    pub git_dir: PathBuf,
}

#[allow(dead_code, reason = "each trust test module uses only some helpers")]
impl GitDir {
    pub fn new(config: &str) -> Self {
        Self::with_bytes(config.as_bytes())
    }

    pub fn with_bytes(config: &[u8]) -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("navo-git-trust-{}-{id}", std::process::id()));
        let git_dir = root.join("repo").join(".git");
        fs::create_dir_all(&git_dir).unwrap();
        fs::create_dir_all(root.join("home")).unwrap();
        fs::write(git_dir.join("config"), config).unwrap();
        Self { root, git_dir }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    pub fn write(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, bytes).unwrap();
        path
    }

    pub fn guard(&self) -> Guard {
        Guard::read_with_home(&self.git_dir, Some(&self.home())).unwrap()
    }
}

impl Drop for GitDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap_or_default();
    }
}

pub fn pairs(guard: &Guard) -> Vec<String> {
    let args = guard.args();
    assert_eq!(args.len() % 2, 0, "args come in -c pairs: {args:?}");
    args.chunks(2)
        .map(|pair| {
            assert_eq!(pair[0], OsString::from("-c"));
            pair[1].to_string_lossy().into_owned()
        })
        .collect()
}

pub fn neutralised(guard: &Guard, name: &str) -> bool {
    let pairs = pairs(guard);
    ["clean=", "smudge=", "process=", "required=false"]
        .iter()
        .all(|tail| pairs.contains(&format!("filter.{name}.{tail}")))
}

/// A Git fixture, or `None` only when Git is not installed; any other setup failure fails the test.
pub fn git_fixture() -> Option<Fixture> {
    match Fixture::new() {
        Ok(fixture) => Some(fixture),
        Err(error) if error.starts_with("git was not found") => None,
        Err(error) => panic!("git fixture setup failed: {error}"),
    }
}
