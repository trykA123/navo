use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, SystemTime};

use tempfile::TempDir;

use super::{Fingerprint, fingerprint};

const STEP: Duration = Duration::from_secs(3600);

struct Repo {
    temp: TempDir,
    newest: SystemTime,
}

impl Repo {
    fn new() -> Self {
        let mut repo = Self {
            temp: TempDir::new().unwrap(),
            newest: SystemTime::now() - Duration::from_secs(86_400),
        };
        repo.mkdir(".git/objects");
        repo.mkdir(".git/refs");
        repo.write(".git/HEAD", "ref: refs/heads/main\n");
        repo.write(".git/config", "[core]\nrepositoryformatversion = 0\n");
        repo.write(".git/index", "index");
        repo.write(".git/refs/heads/main", &format!("{}\n", "a".repeat(40)));
        repo
    }

    fn path(&self) -> &Path {
        self.temp.path()
    }

    fn write(&mut self, relative: &str, text: &str) {
        let path = self.path().join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, text).unwrap();
        self.touch_at(relative, self.newest);
    }

    fn append(&mut self, relative: &str, text: &str) {
        let path = self.path().join(relative);
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
        drop(file);
        self.touch_at(relative, self.newest);
    }

    fn mkdir(&self, relative: &str) {
        fs::create_dir_all(self.path().join(relative)).unwrap();
    }

    fn remove(&self, relative: &str) {
        fs::remove_file(self.path().join(relative)).unwrap();
    }

    fn touch_at(&mut self, relative: &str, at: SystemTime) {
        let path = self.path().join(relative);
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(at)
            .unwrap();
        self.newest = self.newest.max(at);
    }

    fn change(&mut self, relative: &str) {
        if self.path().join(relative).exists() {
            self.touch_at(relative, self.newest + STEP);
        } else {
            self.write(relative, "created");
        }
    }

    fn now(&self) -> SystemTime {
        self.newest + Duration::from_secs(10)
    }

    fn print(&self, upstream: Option<&str>) -> Option<Fingerprint> {
        fingerprint(self.path(), upstream, self.now())
    }
}

#[test]
fn reading_twice_gives_the_same_value() {
    let repo = Repo::new();
    let first = repo.print(None);
    assert!(first.is_some());
    assert_eq!(first, repo.print(None));
}

#[test]
fn a_larger_index_gives_a_different_value() {
    let mut repo = Repo::new();
    let before = repo.print(None);
    repo.append(".git/index", "more");
    assert_ne!(before, repo.print(None));
}

#[test]
fn a_new_index_time_gives_a_different_value() {
    let mut repo = Repo::new();
    let before = repo.print(None);
    repo.touch_at(".git/index", repo.newest + STEP);
    assert_ne!(before, repo.print(None));
}

#[test]
fn another_branch_in_head_gives_a_different_value() {
    let mut repo = Repo::new();
    let before = repo.print(None);
    repo.write(".git/HEAD", "ref: refs/heads/other\n");
    assert_ne!(before, repo.print(None));
}

#[test]
fn a_new_branch_reference_time_gives_a_different_value() {
    let mut repo = Repo::new();
    let before = repo.print(None);
    repo.touch_at(".git/refs/heads/main", repo.newest + STEP);
    assert_ne!(before, repo.print(None));
}

#[test]
fn packed_references_without_a_loose_branch_give_a_different_value() {
    let mut repo = Repo::new();
    repo.remove(".git/refs/heads/main");
    repo.write(
        ".git/packed-refs",
        &format!(
            "# pack-refs with: peeled fully-peeled sorted\n{} refs/heads/main\n",
            "b".repeat(40)
        ),
    );
    let before = repo.print(None);
    assert!(before.is_some());
    repo.append(".git/packed-refs", "#\n");
    assert_ne!(before, repo.print(None));
}

#[test]
fn a_remote_reference_change_gives_a_different_value() {
    let mut repo = Repo::new();
    repo.write(
        ".git/refs/remotes/origin/main",
        &format!("{}\n", "c".repeat(40)),
    );
    let before = repo.print(Some("origin/main"));
    assert!(before.is_some());
    repo.append(".git/refs/remotes/origin/main", "#\n");
    assert_ne!(before, repo.print(Some("origin/main")));
}

#[test]
fn a_local_reference_change_gives_a_different_value() {
    let mut repo = Repo::new();
    let before = repo.print(Some("main"));
    assert!(before.is_some());
    repo.append(".git/refs/heads/main", "#\n");
    assert_ne!(before, repo.print(Some("main")));
}

#[test]
fn fetch_head_gives_a_different_value() {
    let mut repo = Repo::new();
    let before = repo.print(None);
    repo.write(".git/FETCH_HEAD", "0123\t\tbranch 'main' of example\n");
    assert_ne!(before, repo.print(None));
}

#[test]
fn each_marker_gives_a_different_value() {
    for marker in [
        "rebase-merge",
        "rebase-apply",
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "BISECT_LOG",
    ] {
        let mut repo = Repo::new();
        let before = repo.print(None);
        repo.write(&format!(".git/{marker}"), "marker");
        assert_ne!(before, repo.print(None), "{marker}");
    }
}

#[test]
fn config_exclude_and_the_reftable_list_give_a_different_value() {
    for relative in ["config", "info/exclude", "reftable/tables.list"] {
        let mut repo = Repo::new();
        let before = repo.print(None);
        repo.change(&format!(".git/{relative}"));
        assert_ne!(before, repo.print(None), "{relative}");
    }
}

#[test]
fn without_an_upstream_a_remote_reference_change_is_ignored() {
    let mut repo = Repo::new();
    repo.write(
        ".git/refs/remotes/origin/main",
        &format!("{}\n", "d".repeat(40)),
    );
    let before = repo.print(None);
    assert!(before.is_some());
    repo.touch_at(".git/refs/remotes/origin/main", repo.newest + STEP);
    assert_eq!(before, repo.print(None));
}

#[test]
fn a_git_file_gives_none() {
    let temp = TempDir::new().unwrap();
    fs::write(temp.path().join(".git"), "gitdir: ../main/.git\n").unwrap();
    assert_eq!(fingerprint(temp.path(), None, SystemTime::now()), None);
}

#[test]
fn a_commondir_gives_none() {
    let mut repo = Repo::new();
    repo.write(".git/commondir", "../main\n");
    assert_eq!(repo.print(None), None);
}

#[test]
fn a_modules_folder_gives_none() {
    let repo = Repo::new();
    repo.mkdir(".git/modules");
    assert_eq!(repo.print(None), None);
}

#[test]
fn a_gitmodules_file_gives_none() {
    let mut repo = Repo::new();
    repo.write(".gitmodules", "[submodule \"a\"]\n");
    assert_eq!(repo.print(None), None);
}

#[test]
fn an_unsafe_head_reference_gives_none() {
    for head in [
        "ref: refs/../x\n",
        "ref: /x\n",
        "ref: refs\\x\n",
        "ref: refs//x\n",
    ] {
        let mut repo = Repo::new();
        repo.write(".git/HEAD", head);
        assert_eq!(repo.print(None), None, "{head}");
    }
}

#[test]
fn an_unsafe_upstream_gives_none() {
    let repo = Repo::new();
    assert_eq!(repo.print(Some("../x")), None);
    assert_eq!(repo.print(Some("/x")), None);
}

#[test]
fn no_git_folder_gives_none() {
    let temp = TempDir::new().unwrap();
    assert_eq!(fingerprint(temp.path(), None, SystemTime::now()), None);
}

#[test]
fn a_recent_mtime_gives_none() {
    let repo = Repo::new();
    let now = repo.newest + Duration::from_secs(1);
    assert_eq!(
        fingerprint(repo.path(), None, now),
        None,
        "a file written a second before now cannot be confirmed"
    );
}

#[test]
fn a_detached_head_gives_some() {
    let mut repo = Repo::new();
    repo.write(".git/HEAD", &format!("{}\n", "e".repeat(40)));
    assert!(repo.print(None).is_some());
}
