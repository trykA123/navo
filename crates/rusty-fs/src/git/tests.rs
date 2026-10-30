use std::fs;
use std::io::{Read, Write};
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};
use rusty_git::{
    Change, FilterNotice, GitError, GitPath, GitSource, IndexEntry, ObjectId, Others, Target,
    TreeEntry, TreeKind, WorktreeBlob,
};
use tempfile::TempDir;

use crate::{CancelToken, EntryBatch, EntryKind, FsEntry, Provider, ProviderOps};

use super::{GitFs, GitProblem, READ_ONLY, location, provider_id};

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct Fixture {
    _root: TempDir,
    git: PathBuf,
    repo: PathBuf,
}

impl Fixture {
    fn new() -> Result<Self, String> {
        let git = resolve_git(find_git().ok_or_else(|| "git was not found on PATH".to_owned())?);
        let version = version_of(&git)?;
        if version < (2, 36) {
            return Err(format!("git {version:?} is older than 2.36"));
        }
        let root = tempfile::Builder::new()
            .prefix("rusty-fs-git-")
            .tempdir()
            .map_err(|error| error.to_string())?;
        let repo = root.path().join("repo");
        fs::create_dir_all(&repo).map_err(|error| error.to_string())?;
        Ok(Self {
            _root: root,
            git,
            repo,
        })
    }

    fn repo(&self) -> &Path {
        &self.repo
    }

    fn rename_repo(&mut self, name: &str) {
        let renamed = self.repo.with_file_name(name);
        fs::rename(&self.repo, &renamed).unwrap();
        self.repo = renamed;
    }

    fn init(&self) {
        self.git_ok(&["init", "-b", "main"]);
    }

    fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.repo.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    fn git(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }

    fn git_ok(&self, args: &[&str]) -> Output {
        let output = self.git(args);
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn prepare_index_entry(&self, mode: &str, oid: &str, path: &str) {
        self.git_ok(&[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("{mode},{oid},{path}"),
        ]);
    }

    fn add_blob(&self, mode: &str, bytes: &[u8], path: &str) {
        let output = self.git_stdin_ok(&["hash-object", "-w", "--no-filters", "--stdin"], bytes);
        let oid = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        self.prepare_index_entry(mode, &oid, path);
    }

    fn git_stdin_ok(&self, args: &[&str], input: &[u8]) -> Output {
        let mut child = self
            .command()
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(input).unwrap();
        drop(stdin);
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn commit(&self, message: &str) {
        self.git_ok(&["commit", "-m", message]);
    }

    fn head(&self) -> String {
        let output = self.git_ok(&["rev-parse", "HEAD"]);
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.git);
        command.arg("-C").arg(&self.repo).args([
            "-c",
            "user.name=admin",
            "-c",
            "user.email=admin@example.com",
        ]);
        command
    }
}

fn fixture() -> Option<Fixture> {
    match Fixture::new() {
        Ok(fixture) => Some(fixture),
        Err(reason) => {
            let mut stderr = std::io::stderr();
            writeln!(stderr, "skipping: {reason}").unwrap_or_default();
            None
        }
    }
}

fn at(fixture: &Fixture, rev: &str, inner: &str) -> Location {
    location(fixture.repo(), rev, inner).unwrap()
}

fn entries(provider: &GitFs, folder: &Location) -> Vec<FsEntry> {
    let mut entries = Vec::new();
    provider
        .list(
            folder,
            &mut |batch: EntryBatch| {
                entries.extend(batch.entries);
                ControlFlow::Continue(())
            },
            &CancelToken::new(),
        )
        .unwrap();
    entries
}

fn entry<'a>(entries: &'a [FsEntry], name: &str) -> &'a FsEntry {
    entries
        .iter()
        .find(|entry| entry.name.to_text() == name)
        .unwrap_or_else(|| panic!("missing entry {name:?}"))
}

fn read(provider: &GitFs, location: &Location) -> Vec<u8> {
    let mut bytes = Vec::new();
    provider
        .read(location)
        .unwrap()
        .read_to_end(&mut bytes)
        .unwrap();
    bytes
}

#[test]
fn lists_metadata_and_reads_files() {
    let _guard = serial();
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.init();
    fixture.write("file.txt", b"hello\n");
    fixture.write("space name.txt", b"space");
    fixture.write("unicode-\u{00e9}.txt", b"unicode");
    #[cfg(not(windows))]
    fixture.write("line\nbreak.txt", b"newline");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("files");
    let provider = GitFs::new();
    assert_eq!(provider.id(), provider_id());

    let root = at(&fixture, "HEAD", "");
    let listed = entries(&provider, &root);
    let file = entry(&listed, "file.txt");
    assert_eq!(file.kind, EntryKind::File);
    assert_eq!(file.size, 6);
    assert!(file.modified_ms.is_none());
    assert!(file.attributes.readonly);
    let folder = provider.metadata(&root).unwrap();
    assert_eq!(folder.kind, EntryKind::Folder);
    assert!(folder.modified_ms.is_none());

    for path in ["file.txt", "space name.txt", "unicode-\u{00e9}.txt"] {
        let location = at(&fixture, "HEAD", path);
        let metadata = provider.metadata(&location).unwrap();
        assert!(metadata.modified_ms.is_none());
        assert!(!read(&provider, &location).is_empty());
    }
    let file_location = at(&fixture, "HEAD", "file.txt");
    assert_eq!(
        root.join(&ProviderPath::from_text("file.txt")).unwrap(),
        file_location
    );
    assert_eq!(read(&provider, &file_location), b"hello\n");
    let tree_entry = provider.tree_entry(&file_location).unwrap();
    assert_eq!(tree_entry.kind, TreeKind::File);
    assert_eq!(tree_entry.size, Some(6));
    assert_eq!(provider.object_id(&file_location).unwrap(), tree_entry.id);
    #[cfg(not(windows))]
    {
        entry(&listed, "line\nbreak.txt");
        assert_eq!(
            read(&provider, &at(&fixture, "HEAD", "line\nbreak.txt")),
            b"newline"
        );
    }
}

#[test]
fn a_rev_with_a_slash_keeps_the_inner_path() {
    let _guard = serial();
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.init();
    fixture.write("file.txt", b"file\n");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("one");
    fixture.git_ok(&["checkout", "-b", "feature/x"]);
    let provider = GitFs::new();
    let location = at(&fixture, "feature/x", "file.txt");
    assert_eq!(provider.metadata(&location).unwrap().size, 5);
    assert_eq!(read(&provider, &location), b"file\n");
}

#[test]
fn symlinks_are_links_with_their_target() {
    let _guard = serial();
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.init();
    fixture.write("file.txt", b"file\n");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("file");
    fixture.add_blob("120000", b"file.txt", "link.txt");
    fixture.commit("link");
    let provider = GitFs::new();

    let listed = entries(&provider, &at(&fixture, "HEAD", ""));
    let link = entry(&listed, "link.txt");
    assert_eq!(link.kind, EntryKind::Link);
    assert_eq!(link.size, 8);
    let location = at(&fixture, "HEAD", "link.txt");
    assert_eq!(provider.metadata(&location).unwrap().kind, EntryKind::Link);
    assert_eq!(read(&provider, &location), b"file.txt");
    assert_eq!(
        provider.link_target(&location).unwrap(),
        Some(at(&fixture, "HEAD", "file.txt"))
    );
}

#[test]
fn submodule_entries_are_unavailable() {
    let _guard = serial();
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.init();
    fixture.write("normal.txt", b"normal\n");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("base");
    let head = fixture.head();
    fixture.prepare_index_entry("160000", &head, "vendor/lib");
    fixture.commit("submodule");
    let provider = GitFs::new();

    let root = entries(&provider, &at(&fixture, "HEAD", ""));
    assert_eq!(entry(&root, "vendor").kind, EntryKind::Folder);
    let vendor = entries(&provider, &at(&fixture, "HEAD", "vendor"));
    assert_eq!(entry(&vendor, "lib").kind, EntryKind::Folder);
    let lib = at(&fixture, "HEAD", "vendor/lib");
    assert_eq!(
        provider.metadata(&lib).unwrap_err().kind,
        FsErrorKind::NotSupportedYet
    );
    assert_eq!(
        provider.tree_entry(&lib).unwrap_err().kind,
        FsErrorKind::NotSupportedYet
    );
    assert_eq!(
        provider.read(&lib).err().unwrap().kind,
        FsErrorKind::NotSupportedYet
    );
}

#[test]
fn an_unknown_rev_is_not_found() {
    let _guard = serial();
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.init();
    fixture.write("file.txt", b"file\n");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("one");
    let provider = GitFs::new();
    let missing = at(&fixture, "missing-rev", "");
    assert_eq!(
        provider.metadata(&missing).unwrap_err().kind,
        FsErrorKind::NotFound
    );
    assert_eq!(
        provider
            .list(
                &missing,
                &mut |_: EntryBatch| ControlFlow::Continue(()),
                &CancelToken::new()
            )
            .unwrap_err()
            .kind,
        FsErrorKind::NotFound
    );
    assert!(matches!(
        provider.last_problem(&missing),
        Some(GitProblem::RevNotFound { .. })
    ));
}

fn refused(result: Result<impl Sized, FsError>) {
    let error = result.err().expect("the write was accepted");
    assert_eq!(error.kind, FsErrorKind::NotSupportedYet);
    assert_eq!(READ_ONLY, "Git refs are read-only");
}

#[test]
fn every_write_is_refused() {
    let _guard = serial();
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.init();
    fixture.write("file.txt", b"file\n");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("one");
    let provider = GitFs::new();
    let file = at(&fixture, "HEAD", "file.txt");
    let folder = at(&fixture, "HEAD", "");

    refused(provider.create_file(&file));
    refused(provider.create_folder(&folder));
    refused(provider.create_private_folder(&folder));
    refused(provider.remove_file(&file));
    refused(provider.remove_link(&file));
    refused(provider.remove_empty_folder(&folder));
    refused(provider.recycle(&file));
    refused(provider.rename(&file, &folder));
    refused(provider.replace(&file, &folder));
    refused(provider.rename_atomic(&file, &folder));
    refused(
        provider
            .copy_file(&file, &folder, &mut |_| {}, &CancelToken::new())
            .map(|_| ()),
    );
    refused(provider.make_link(&file, &folder));
    refused(provider.restrict_to_owner(&file));
    assert!(matches!(
        provider.last_problem(&file),
        Some(GitProblem::ReadOnly)
    ));
}

struct FakeGit {
    id: ObjectId,
    calls: Arc<AtomicUsize>,
    resolves: Arc<AtomicUsize>,
    listed_rev: String,
    entries: Vec<TreeEntry>,
}

impl GitSource for FakeGit {
    fn resolve(&self, _name: &str) -> Result<Option<ObjectId>, GitError> {
        self.resolves.fetch_add(1, Ordering::Relaxed);
        Ok(Some(self.id))
    }

    fn refs(&self) -> Result<Vec<rusty_git::GitRef>, GitError> {
        Ok(Vec::new())
    }

    fn list_tree(&self, rev: &str) -> Result<Vec<TreeEntry>, GitError> {
        assert_eq!(rev, self.listed_rev);
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(self.entries.clone())
    }

    fn read_blob(&self, _id: &ObjectId) -> Result<rusty_git::BlobReader, GitError> {
        Err(unsupported())
    }

    fn read_blob_worktree_form(
        &self,
        _rev: &str,
        _path: &GitPath,
    ) -> Result<WorktreeBlob, GitError> {
        Err(unsupported())
    }

    fn changes(&self, _a: &str, _target: Target<'_>) -> Result<Vec<Change>, GitError> {
        Err(unsupported())
    }

    fn merge_base(&self, _a: &str, _b: &str) -> Result<Option<ObjectId>, GitError> {
        Ok(None)
    }

    fn index_entries(&self, _stage: rusty_git::Stage) -> Result<Vec<IndexEntry>, GitError> {
        Err(unsupported())
    }

    fn others(&self, _which: Others) -> Result<Vec<rusty_git::OtherPath>, GitError> {
        Err(unsupported())
    }
}

fn unsupported() -> GitError {
    GitError::Unsupported {
        operation: "test fake".to_owned(),
    }
}

#[test]
fn a_tree_is_listed_once_per_commit() {
    let _guard = serial();
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.init();
    let id = ObjectId::Sha1([7; 20]);
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let resolves = Arc::new(AtomicUsize::new(0));
    let resolve_counter = Arc::clone(&resolves);
    let expected = "07".repeat(20);
    let tree = vec![TreeEntry {
        path: GitPath::from("file.txt"),
        id,
        kind: TreeKind::File,
        size: Some(6),
    }];
    let provider = GitFs::with_factory(move |_| {
        Ok(Box::new(FakeGit {
            id,
            calls: Arc::clone(&counter),
            resolves: Arc::clone(&resolve_counter),
            listed_rev: expected.clone(),
            entries: tree.clone(),
        }))
    });

    let main = entries(&provider, &at(&fixture, "main", ""));
    assert_eq!(main[0].size, 6);
    let alias = entries(&provider, &at(&fixture, "alias", ""));
    assert_eq!(alias[0].size, 6);
    provider
        .metadata(&at(&fixture, "main", "file.txt"))
        .unwrap();
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert_eq!(resolves.load(Ordering::Relaxed), 2);
}

fn error_kind(fixture: &Fixture, error: fn(&Path) -> GitError) -> FsErrorKind {
    let provider = GitFs::with_factory(move |path| Err(error(path)));
    let location = at(fixture, "HEAD", "");
    provider
        .list(
            &location,
            &mut |_: EntryBatch| ControlFlow::Continue(()),
            &CancelToken::new(),
        )
        .unwrap_err()
        .kind
}

#[test]
fn git_errors_keep_their_kind() {
    let _guard = serial();
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.init();
    assert_eq!(
        error_kind(&fixture, |path| GitError::UnsafeDirectory {
            path: path.to_path_buf()
        }),
        FsErrorKind::AccessDenied
    );
    assert_eq!(
        error_kind(&fixture, |path| GitError::NotARepository {
            path: path.to_path_buf()
        }),
        FsErrorKind::NotFound
    );
    assert_eq!(
        error_kind(&fixture, |_| GitError::Unsupported {
            operation: "test".to_owned()
        }),
        FsErrorKind::NotSupportedYet
    );
    assert_eq!(
        error_kind(&fixture, |_| GitError::Timeout),
        FsErrorKind::Timeout
    );
}

#[test]
fn a_tag_and_a_slash_branch_are_distinct() {
    let _guard = serial();
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.init();
    fixture.write("file.txt", b"tag");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("tag");
    fixture.git_ok(&["tag", "feature"]);
    fixture.git_ok(&["checkout", "-b", "feature/x"]);
    fixture.write("file.txt", b"branch");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("branch");
    let provider = GitFs::new();

    let tag = at(&fixture, "feature", "file.txt");
    let branch = at(&fixture, "feature/x", "file.txt");
    assert_ne!(tag, branch);
    assert_eq!(read(&provider, &tag), b"tag");
    assert_eq!(read(&provider, &branch), b"branch");
}

#[test]
fn a_symlink_target_stays_inside_the_tree() {
    let _guard = serial();
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.init();
    fixture.write("dir@HEAD/secret.txt", b"secret");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("secret");
    fixture.add_blob("120000", b"dir@HEAD/secret.txt", "link");
    fixture.commit("link");
    let provider = GitFs::new();
    let link = at(&fixture, "HEAD", "link");

    let target = provider.link_target(&link).unwrap().unwrap();
    assert_eq!(target, at(&fixture, "HEAD", "dir@HEAD/secret.txt"));
    assert_eq!(read(&provider, &target), b"secret");
}

#[test]
fn a_repository_path_with_an_at_is_encoded() {
    let _guard = serial();
    let Some(mut fixture) = fixture() else {
        return;
    };
    fixture.rename_repo("re@po");
    fixture.init();
    fixture.write("file.txt", b"file");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("one");
    let provider = GitFs::new();
    let location = at(&fixture, "HEAD", "file.txt");
    assert!(location.as_str().contains("%40"));
    assert_eq!(read(&provider, &location), b"file");
}

#[test]
fn refresh_clears_resolved_revs() {
    let _guard = serial();
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.init();
    fixture.write("a.txt", b"a");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("a");
    let provider = GitFs::new();
    let root = at(&fixture, "main", "");
    assert_eq!(entry(&entries(&provider, &root), "a.txt").size, 1);

    fixture.write("b.txt", b"b");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("b");
    assert!(
        entries(&provider, &root)
            .iter()
            .all(|entry| entry.name.to_text() != "b.txt")
    );

    provider.refresh();
    assert_eq!(entry(&entries(&provider, &root), "b.txt").size, 1);
}

#[test]
fn git_errors_keep_their_details() {
    let _guard = serial();
    let Some(fixture) = fixture() else {
        return;
    };
    let repo = fixture.repo().to_path_buf();
    let provider = GitFs::with_factory(move |path| {
        Err(GitError::UnsafeDirectory {
            path: path.to_path_buf(),
        })
    });
    let location = at(&fixture, "HEAD", "");
    let error = provider
        .list(
            &location,
            &mut |_: EntryBatch| ControlFlow::Continue(()),
            &CancelToken::new(),
        )
        .unwrap_err();
    assert_eq!(error.kind, FsErrorKind::AccessDenied);
    match provider.last_problem(&location) {
        Some(GitProblem::Git(GitError::UnsafeDirectory { path })) => assert_eq!(path, repo),
        other => panic!("unexpected problem: {other:?}"),
    }
}

#[test]
fn filter_notices_are_kept_for_successful_reads() {
    let _guard = serial();
    let Some(fixture) = fixture() else {
        return;
    };
    fixture.init();
    fixture.write(".gitattributes", b"*.txt filter=custom -text\n");
    fixture.write("file.txt", b"raw\n");
    fixture.git_ok(&["add", "-A"]);
    fixture.commit("one");
    fixture.git_ok(&[
        "config",
        "filter.custom.smudge",
        "git-filter-command-that-does-not-exist",
    ]);
    let provider = GitFs::new();
    let location = at(&fixture, "HEAD", "file.txt");
    assert_eq!(read(&provider, &location), b"raw\n");
    match provider.last_problem(&location) {
        Some(GitProblem::Filter(FilterNotice::Filter { detail })) => {
            assert!(!detail.trim().is_empty());
        }
        other => panic!("unexpected problem: {other:?}"),
    }
}

#[test]
fn an_overlong_location_is_rejected_without_git() {
    let _guard = serial();
    let text = format!("{}@HEAD", "a".repeat(super::location::MAX_LOCATION_BYTES));
    let location =
        Location::with_provider(provider_id(), rusty_core::ProviderPath::from_text(&text)).unwrap();
    let provider = GitFs::new();
    let error = provider
        .list(
            &location,
            &mut |_: EntryBatch| ControlFlow::Continue(()),
            &CancelToken::new(),
        )
        .unwrap_err();
    assert_eq!(error.kind, FsErrorKind::PathTooLong);
    assert!(matches!(
        provider.last_problem(&location),
        Some(GitProblem::LocationTooLong { .. })
    ));
}

fn resolve_git(git: PathBuf) -> PathBuf {
    let Ok(output) = Command::new(&git).arg("--exec-path").output() else {
        return git;
    };
    if !output.status.success() {
        return git;
    }
    let exec_path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if exec_path.is_empty() {
        return git;
    }
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let candidate = Path::new(&exec_path).join(name);
    if fs::metadata(&candidate).is_ok_and(|metadata| metadata.is_file()) {
        candidate
    } else {
        git
    }
}

fn find_git() -> Option<PathBuf> {
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|folder| folder.join(name))
        .find(|candidate| fs::metadata(candidate).is_ok_and(|metadata| metadata.is_file()))
}

fn version_of(git: &Path) -> Result<(u32, u32), String> {
    let output = Command::new(git)
        .arg("--version")
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err("git --version failed".to_owned());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let token = text
        .split_whitespace()
        .find(|token| token.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .ok_or_else(|| "git version was not printed".to_owned())?;
    let mut parts = token.split('.');
    let major = number(parts.next()).ok_or_else(|| "git major version".to_owned())?;
    let minor = number(parts.next()).ok_or_else(|| "git minor version".to_owned())?;
    Ok((major, minor))
}

fn number(part: Option<&str>) -> Option<u32> {
    let digits: String = part?
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}
