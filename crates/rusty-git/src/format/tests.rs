use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::GitError;

use super::{ObjectFormat, RefStorage, RepoFormat};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("rusty-git-format-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.path.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    fn mkdir(&self, relative: &str) {
        fs::create_dir_all(self.path.join(relative)).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap_or_default();
    }
}

fn git_dir(temp: &TempDir, relative: &str, config: &str) {
    temp.write(&format!("{relative}/HEAD"), "ref: refs/heads/main\n");
    temp.write(&format!("{relative}/config"), config);
    temp.mkdir(&format!("{relative}/objects"));
    temp.mkdir(&format!("{relative}/refs"));
}

fn files_repo(temp: &TempDir, config: &str) {
    git_dir(temp, ".git", config);
}

fn version(version: &str) -> String {
    format!("[core]\nrepositoryformatversion = {version}\n")
}

#[test]
fn a_sha1_files_repository_is_read() {
    let temp = TempDir::new();
    files_repo(
        &temp,
        "[Core]\n\
         \tBARE = \"false\" ; a quoted boolean with a comment\n\
         \trepositoryformatversion = 0\n\
         [remote \"origin\"]\n\
         \turl = \"https://example.invalid/a#b\"\n",
    );
    let format = RepoFormat::read(temp.path()).unwrap();
    assert_eq!(format.object_format, ObjectFormat::Sha1);
    assert_eq!(format.ref_storage, RefStorage::Files);
    assert!(!format.bare);
    assert_eq!(format.version, 0);
    assert_eq!(format.git_dir, temp.path().join(".git"));
    assert_eq!(format.common_dir, temp.path().join(".git"));
}

#[test]
fn a_sha256_repository_is_read() {
    let temp = TempDir::new();
    files_repo(
        &temp,
        &format!("{}[extensions]\nobjectFormat = sha256\n", version("1")),
    );
    let format = RepoFormat::read(temp.path()).unwrap();
    assert_eq!(format.object_format, ObjectFormat::Sha256);
    assert_eq!(format.ref_storage, RefStorage::Files);
    assert_eq!(format.version, 1);
}

#[test]
fn a_reftable_repository_is_read() {
    let temp = TempDir::new();
    files_repo(
        &temp,
        &format!("{}[extensions]\nrefStorage = \"reftable\"\n", version("1")),
    );
    let format = RepoFormat::read(temp.path()).unwrap();
    assert_eq!(format.object_format, ObjectFormat::Sha1);
    assert_eq!(format.ref_storage, RefStorage::Reftable);
}

#[test]
fn a_continuation_line_completes_the_object_format() {
    let temp = TempDir::new();
    files_repo(
        &temp,
        &format!("{}[extensions]\nobjectFormat = sha\\\n256\n", version("1")),
    );
    let format = RepoFormat::read(temp.path()).unwrap();
    assert_eq!(format.object_format, ObjectFormat::Sha256);
}

#[test]
fn the_version_one_extension_list_is_accepted() {
    let temp = TempDir::new();
    files_repo(
        &temp,
        &format!(
            "{}[extensions]\n\
             noop = true\n\
             noop-v1 = true\n\
             preciousObjects = true\n\
             partialClone = origin\n\
             worktreeConfig = false\n\
             objectFormat = sha256\n\
             compatObjectFormat = sha1\n\
             refStorage = files\n\
             relativeWorktrees = true\n\
             submodulePathConfig = true\n",
            version("1")
        ),
    );
    let format = RepoFormat::read(temp.path()).unwrap();
    assert_eq!(format.object_format, ObjectFormat::Sha256);
    assert_eq!(format.ref_storage, RefStorage::Files);
}

#[test]
fn the_version_zero_extension_list_is_accepted() {
    let temp = TempDir::new();
    files_repo(
        &temp,
        &format!(
            "{}[extensions]\n\
             noop = true\n\
             preciousObjects = true\n\
             partialClone = origin\n\
             worktreeConfig = false\n",
            version("0")
        ),
    );
    let format = RepoFormat::read(temp.path()).unwrap();
    assert_eq!(format.object_format, ObjectFormat::Sha1);
    assert_eq!(format.ref_storage, RefStorage::Files);
}

#[test]
fn version_one_only_extensions_are_rejected_at_version_zero() {
    for (name, value) in [
        ("noop-v1", "true"),
        ("objectFormat", "sha256"),
        ("compatObjectFormat", "sha256"),
        ("refStorage", "files"),
        ("relativeWorktrees", "true"),
        ("submodulePathConfig", "true"),
    ] {
        let temp = TempDir::new();
        files_repo(
            &temp,
            &format!("{}[extensions]\n{name} = {value}\n", version("0")),
        );
        let error = RepoFormat::read(temp.path()).unwrap_err();
        assert!(matches!(error, GitError::Unsupported { .. }), "{name}");
    }
}

#[test]
fn a_linked_worktree_reads_its_common_config() {
    let temp = TempDir::new();
    linked_worktree(
        &temp,
        &format!("{}[extensions]\nobjectFormat = sha256\n", version("1")),
    );
    let format = RepoFormat::read(&temp.path().join("wt")).unwrap();
    assert_eq!(format.object_format, ObjectFormat::Sha256);
    assert!(!format.bare);
    assert_eq!(
        format.git_dir,
        temp.path().join("wt/../main/.git/worktrees/wt")
    );
    assert_eq!(
        format.common_dir,
        temp.path().join("wt/../main/.git/worktrees/wt/../..")
    );
}

fn linked_worktree(temp: &TempDir, config: &str) {
    temp.write("main/.git/HEAD", "ref: refs/heads/main\n");
    temp.write("main/.git/config", config);
    temp.mkdir("main/.git/objects");
    temp.mkdir("main/.git/refs");
    temp.write("main/.git/worktrees/wt/HEAD", "ref: refs/heads/main\n");
    temp.write("main/.git/worktrees/wt/commondir", "../..\n");
    temp.write("wt/.git", "gitdir: ../main/.git/worktrees/wt\n");
}

#[test]
fn a_linked_worktree_ignores_common_bareness() {
    let temp = TempDir::new();
    linked_worktree(&temp, "[core]\nrepositoryformatversion = 0\nbare = true\n");
    let format = RepoFormat::read(&temp.path().join("wt")).unwrap();
    assert!(!format.bare);
}

#[test]
fn linked_worktree_config_can_set_bare() {
    let temp = TempDir::new();
    linked_worktree(
        &temp,
        &format!("{}[extensions]\nworktreeConfig = true\n", version("1")),
    );
    temp.write(
        "main/.git/worktrees/wt/config.worktree",
        "[core]\nbare = true\n",
    );
    let format = RepoFormat::read(&temp.path().join("wt")).unwrap();
    assert!(format.bare);
}

#[test]
fn an_absolute_gitdir_file_is_read() {
    let temp = TempDir::new();
    let git_dir = temp.path().join("main/.git/worktrees/wt");
    linked_worktree(&temp, &version("0"));
    temp.write("wt/.git", &format!("gitdir: {}\n", git_dir.display()));
    let format = RepoFormat::read(&temp.path().join("wt")).unwrap();
    assert_eq!(format.git_dir, git_dir);
    assert_eq!(format.object_format, ObjectFormat::Sha1);
}

#[test]
fn a_git_file_without_commondir_is_a_separate_git_dir() {
    let temp = TempDir::new();
    git_dir(
        &temp,
        "separate",
        "[core]\nrepositoryformatversion = 0\nbare = true\n",
    );
    temp.write(".git", "gitdir: separate\n");
    let format = RepoFormat::read(temp.path()).unwrap();
    assert!(format.bare);
    assert_eq!(format.common_dir, temp.path().join("separate"));
}

#[test]
fn a_bare_repository_is_read() {
    let temp = TempDir::new();
    git_dir(
        &temp,
        "bare",
        "[core]\nbare = true\nrepositoryformatversion = 0\n",
    );
    let format = RepoFormat::read(&temp.path().join("bare")).unwrap();
    assert!(format.bare);
    assert_eq!(format.git_dir, temp.path().join("bare"));
    assert_eq!(format.common_dir, temp.path().join("bare"));
}

#[test]
fn an_absent_bare_value_uses_the_git_dir_name() {
    let normal = TempDir::new();
    files_repo(&normal, &version("0"));
    assert!(!RepoFormat::read(normal.path()).unwrap().bare);

    let bare = TempDir::new();
    git_dir(&bare, "bare", &version("0"));
    assert!(RepoFormat::read(&bare.path().join("bare")).unwrap().bare);

    let separate = TempDir::new();
    git_dir(&separate, "repo.git", &version("0"));
    separate.write(".git", "gitdir: repo.git\n");
    assert!(RepoFormat::read(separate.path()).unwrap().bare);
}

#[test]
fn a_git_dir_must_have_head_objects_and_refs() {
    for missing in ["HEAD", "objects", "refs"] {
        let temp = TempDir::new();
        files_repo(&temp, &version("0"));
        let path = temp.path().join(".git").join(missing);
        if missing == "HEAD" {
            fs::remove_file(path).unwrap();
        } else {
            fs::remove_dir_all(path).unwrap();
        }
        let error = RepoFormat::read(temp.path()).unwrap_err();
        assert!(
            matches!(error, GitError::NotARepository { .. }),
            "{missing}"
        );
    }
}

#[test]
fn a_broken_git_file_is_an_error() {
    let temp = TempDir::new();
    temp.write(".git", "this is not a gitdir line\n");
    let error = RepoFormat::read(temp.path()).unwrap_err();
    assert!(matches!(error, GitError::InvalidRepositoryFile { .. }));
}

#[test]
fn repository_format_version_two_is_unsupported() {
    let temp = TempDir::new();
    files_repo(&temp, &version("2"));
    let error = RepoFormat::read(temp.path()).unwrap_err();
    assert!(matches!(error, GitError::Unsupported { .. }));
}

#[test]
fn an_unknown_extension_at_version_one_is_unsupported() {
    let temp = TempDir::new();
    files_repo(
        &temp,
        &format!("{}[extensions]\nfutureThing = true\n", version("1")),
    );
    let error = RepoFormat::read(temp.path()).unwrap_err();
    assert!(matches!(
        error,
        GitError::Unsupported { operation } if operation.contains("futureThing")
    ));
}

#[test]
fn an_extension_in_a_subsection_is_unknown() {
    let temp = TempDir::new();
    files_repo(
        &temp,
        &format!(
            "{}[extensions \"future\"]\nobjectFormat = sha256\n",
            version("1")
        ),
    );
    let error = RepoFormat::read(temp.path()).unwrap_err();
    assert!(matches!(
        error,
        GitError::Unsupported { operation } if operation.contains("future")
    ));
}

#[test]
fn object_format_at_version_zero_is_unsupported() {
    let temp = TempDir::new();
    files_repo(
        &temp,
        &format!("{}[extensions]\nobjectFormat = sha256\n", version("0")),
    );
    let error = RepoFormat::read(temp.path()).unwrap_err();
    assert!(matches!(error, GitError::Unsupported { .. }));
}

#[test]
fn an_uppercase_object_format_is_an_error() {
    let temp = TempDir::new();
    files_repo(
        &temp,
        &format!("{}[extensions]\nobjectFormat = SHA256\n", version("1")),
    );
    let error = RepoFormat::read(temp.path()).unwrap_err();
    assert!(matches!(
        error,
        GitError::UnsupportedObjectFormat { value } if value == "SHA256"
    ));
}

#[test]
fn an_unknown_object_format_is_an_error() {
    let temp = TempDir::new();
    files_repo(
        &temp,
        &format!("{}[extensions]\nobjectFormat = md5\n", version("1")),
    );
    let error = RepoFormat::read(temp.path()).unwrap_err();
    assert!(matches!(
        error,
        GitError::UnsupportedObjectFormat { value } if value == "md5"
    ));
}

#[test]
fn worktree_config_can_set_bare_at_version_zero() {
    let temp = TempDir::new();
    files_repo(
        &temp,
        &format!("{}[extensions]\nworktreeConfig = true\n", version("0")),
    );
    temp.write(".git/config.worktree", "[core]\nbare = true\n");
    let format = RepoFormat::read(temp.path()).unwrap();
    assert!(format.bare);
}

#[test]
fn an_invalid_worktree_config_boolean_is_an_error() {
    let temp = TempDir::new();
    files_repo(
        &temp,
        &format!("{}[extensions]\nworktreeConfig = true\n", version("1")),
    );
    temp.write(".git/config.worktree", "[core]\nbare = maybe\n");
    let error = RepoFormat::read(temp.path()).unwrap_err();
    assert!(matches!(error, GitError::InvalidConfig { .. }));
}

#[test]
fn an_empty_bare_value_is_false() {
    let temp = TempDir::new();
    files_repo(&temp, "[core]\nbare =\nrepositoryformatversion = 0\n");
    let format = RepoFormat::read(temp.path()).unwrap();
    assert!(!format.bare);
}

#[test]
fn a_numeric_bare_value_is_true() {
    let temp = TempDir::new();
    files_repo(&temp, "[core]\nbare = 2\nrepositoryformatversion = 0\n");
    let format = RepoFormat::read(temp.path()).unwrap();
    assert!(format.bare);
}

#[cfg(unix)]
#[test]
fn a_symlink_loop_on_config_is_an_io_error() {
    use std::os::unix::fs::symlink;

    let temp = TempDir::new();
    temp.write(".git/HEAD", "ref: refs/heads/main\n");
    temp.mkdir(".git/objects");
    temp.mkdir(".git/refs");
    symlink("config", temp.path().join(".git/config")).unwrap();
    let error = RepoFormat::read(temp.path()).unwrap_err();
    assert!(matches!(error, GitError::Io { .. }), "{error:?}");
}

#[cfg(unix)]
#[test]
fn a_non_utf8_gitdir_is_read() {
    use std::ffi::OsString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let temp = TempDir::new();
    let git_dir = temp
        .path()
        .join(OsString::from_vec(b"git-dir-\xff".to_vec()));
    fs::create_dir_all(git_dir.join("objects")).unwrap();
    fs::create_dir_all(git_dir.join("refs")).unwrap();
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(git_dir.join("config"), version("0")).unwrap();
    let mut content = b"gitdir: ".to_vec();
    content.extend_from_slice(git_dir.as_os_str().as_bytes());
    content.push(b'\n');
    fs::write(temp.path().join(".git"), content).unwrap();
    let format = RepoFormat::read(temp.path()).unwrap();
    assert_eq!(format.git_dir, git_dir);
}

#[cfg(unix)]
#[test]
fn commondir_keeps_trailing_spaces_and_strips_only_the_newline() {
    let temp = TempDir::new();
    let common = temp.path().join("common ");
    fs::create_dir_all(common.join("objects")).unwrap();
    fs::create_dir_all(common.join("refs")).unwrap();
    fs::write(common.join("config"), version("0")).unwrap();
    let git_dir = temp.path().join("main/.git/worktrees/wt");
    fs::create_dir_all(&git_dir).unwrap();
    fs::write(git_dir.join("HEAD"), "ref: refs/heads/main\n").unwrap();
    let mut target = common.as_os_str().as_encoded_bytes().to_vec();
    target.extend_from_slice(b"\r\n");
    fs::write(git_dir.join("commondir"), target).unwrap();
    temp.write("wt/.git", &format!("gitdir: {}\n", git_dir.display()));
    let format = RepoFormat::read(&temp.path().join("wt")).unwrap();
    assert_eq!(format.common_dir, common);
}
