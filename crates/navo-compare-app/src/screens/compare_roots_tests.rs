//! Tests for `roots_of`: how two sides, paths or Git revisions, become a run kind and two roots.

use std::path::{Path, PathBuf};

use navo_compare_cli::args::SideSpec;

use super::compare_roots::roots_of;
use super::compare_run::{RunError, RunKind};
use crate::start::Side;

/// A unique folder under the system temp root, removed when dropped.
struct Dir(PathBuf);

impl Dir {
    /// Creates the folder with `tag` and the process id in its name.
    fn new(tag: &str) -> Self {
        let path = std::env::temp_dir().join(format!("sidewise-g7-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    /// The folder path.
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Dir {
    /// Removes the folder and its files.
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A Git side for `repo` at `main`.
fn git(repo: &Path) -> SideSpec {
    SideSpec::GitRev {
        repo: repo.to_path_buf(),
        rev: "main".into(),
    }
}

/// Two Git sides are two folder roots in the `git` provider.
#[test]
fn git_and_git_are_folders_with_git_locations() {
    let dir = Dir::new("gg");
    let (kind, roots) = roots_of(&git(dir.path()), &git(dir.path())).expect("roots");
    assert_eq!(kind, RunKind::Folders);
    assert!(roots.iter().all(|root| root.as_str().starts_with("git:")));
}

/// A Git side and a folder are folders; the folder root stays local.
#[test]
fn git_and_folder_are_folders() {
    let dir = Dir::new("gf");
    let right = SideSpec::Path(dir.path().to_path_buf());
    let (kind, roots) = roots_of(&git(dir.path()), &right).expect("roots");
    assert_eq!(kind, RunKind::Folders);
    assert!(roots[0].as_str().starts_with("git-checkout:"));
    assert!(roots[1].is_local());
}

/// A folder and a Git side: the Git root on the right is the checkout form.
#[test]
fn folder_and_git_gives_the_right_side_checkout() {
    let dir = Dir::new("fg");
    let left = SideSpec::Path(dir.path().to_path_buf());
    let (kind, roots) = roots_of(&left, &git(dir.path())).expect("roots");
    assert_eq!(kind, RunKind::Folders);
    assert!(roots[0].is_local());
    assert!(roots[1].as_str().starts_with("git-checkout:"));
}

/// A local revision against a remote one keeps the stored form; the remote stays `git-remote`.
#[test]
fn git_and_remote_are_both_stored() {
    let dir = Dir::new("gr");
    let right = SideSpec::GitUrl {
        url: "https://github.com/owner/repo".into(),
        rev: "main".into(),
    };
    let (_, roots) = roots_of(&git(dir.path()), &right).expect("roots");
    assert!(roots[0].as_str().starts_with("git:"));
    assert!(roots[1].as_str().starts_with("git-remote:"));
}

/// A Git side and a file cannot be compared.
#[test]
fn git_and_file_are_mixed() {
    let dir = Dir::new("gx");
    let file = dir.path().join("a.txt");
    std::fs::write(&file, "a").expect("write");
    let result = roots_of(&git(dir.path()), &SideSpec::Path(file));
    assert_eq!(result.map(|(kind, _)| kind), Err(RunError::Mixed));
}

/// A missing path beside a Git side names its own side.
#[test]
fn git_and_missing_names_the_missing_side() {
    let dir = Dir::new("gm");
    let missing = SideSpec::Path(dir.path().join("none"));
    let result = roots_of(&git(dir.path()), &missing);
    assert_eq!(
        result.map(|(kind, _)| kind),
        Err(RunError::Missing(Side::Right))
    );
}

/// A folder beside a missing path reports the right side.
#[test]
fn folder_and_missing_is_missing_right() {
    let dir = Dir::new("fm");
    let left = SideSpec::Path(dir.path().to_path_buf());
    let right = SideSpec::Path(dir.path().join("none"));
    let result = roots_of(&left, &right);
    assert_eq!(
        result.map(|(kind, _)| kind),
        Err(RunError::Missing(Side::Right))
    );
}

/// A remote Git side's root is the `git-remote` location that `at::parse` reads back.
#[test]
fn a_remote_side_encodes_a_git_remote_location() {
    let remote = SideSpec::GitUrl {
        url: "https://github.com/owner/repo".into(),
        rev: "main".into(),
    };
    let dir = Dir::new("rm");
    let (kind, [left, _]) =
        roots_of(&remote, &SideSpec::Path(dir.path().to_path_buf())).expect("roots");
    assert_eq!(kind, RunKind::Folders);
    let at = sidewise_git_remote::at::parse(&left).expect("parses back");
    assert_eq!(at.url.as_str(), "https://github.com/owner/repo");
    assert_eq!((at.rev.as_str(), at.inner.as_str()), ("main", ""));
}

/// An address the parser refuses is a `Location` error naming the side's problem.
#[test]
fn a_bad_remote_address_is_a_location_error() {
    let remote = SideSpec::GitUrl {
        url: "not an address".into(),
        rev: "main".into(),
    };
    let dir = Dir::new("rb");
    let result = roots_of(&remote, &SideSpec::Path(dir.path().to_path_buf()));
    assert!(matches!(result, Err(RunError::Location(_))));
}
