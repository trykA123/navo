//! Opens a local Git repository and resolves a revision to one commit, in process with gix.
//! Read-only; never starts a program. Every call blocks, so callers stay off the UI thread.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use gix::error::Class;
use sidewise_engine::model::{ObjectId, Reason};

/// Largest object read from a repository, in bytes (64 MiB).
pub const MAX_BLOB: u64 = 64 * 1024 * 1024;

/// Why a repository or a revision cannot be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitProblem {
    /// The folder is not a repository (worktree root, `.git` folder or bare repository).
    NotRepository,
    /// The revision names no single commit.
    NoSuchRevision,
    /// The repository uses a format Sidewise cannot read, such as SHA-256.
    Unsupported,
    /// The repository could not be read: damaged data, an I/O error or a parser panic.
    Broken,
}

impl GitProblem {
    /// The compare reason shown for a side with this problem.
    #[must_use]
    pub const fn reason(self) -> Reason {
        match self {
            Self::NotRepository | Self::NoSuchRevision => Reason::NotFound,
            Self::Unsupported => Reason::Unsupported,
            Self::Broken => Reason::ReadError,
        }
    }

    /// The l10n id of the Start chip text for this problem.
    #[must_use]
    pub const fn l10n_id(self) -> &'static str {
        match self {
            Self::NotRepository => "git-chip-not-repository",
            Self::NoSuchRevision => "git-chip-no-revision",
            Self::Unsupported => "git-chip-unsupported",
            Self::Broken => "git-chip-broken",
        }
    }
}

/// A revision resolved to one commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    /// The commit id, as the engine stores it (always SHA-1).
    pub id: ObjectId,
    /// The id of the commit's root tree.
    pub tree: gix::ObjectId,
    /// The first 7 lowercase hex digits of the commit id.
    pub short: String,
}

/// An open repository, shared between threads (gix's `parallel` feature makes it `Send + Sync`).
#[derive(Clone)]
pub struct Repo {
    shared: gix::ThreadSafeRepository,
    path: PathBuf,
}

impl Repo {
    /// Opens the repository at `path` (worktree root, `.git` folder or bare repository) with
    /// the user's global Git config, not the system one. Fails with `NotRepository`,
    /// `Unsupported` or `Broken`.
    pub fn open(path: &Path) -> Result<Repo, GitProblem> {
        Self::open_with(path, gix::open::Permissions::default())
    }

    /// Opens the repository at `path` with only its own config, for tests.
    #[cfg(any(test, feature = "test-support"))]
    pub fn open_isolated(path: &Path) -> Result<Repo, GitProblem> {
        Self::open_with(path, gix::open::Permissions::isolated())
    }

    /// Opens `path` without the owner check (reads run no program), objects capped at
    /// `MAX_BLOB`, no config source that needs the `git` program to locate it, and no
    /// `include`/`includeIf` files (a hostile one can name an SMB share). Pointer files that
    /// name a network path are refused before gix reads them (`pointers::check`).
    fn open_with(path: &Path, mut permissions: gix::open::Permissions) -> Result<Repo, GitProblem> {
        // On Windows, gix finds the system config by running `git config --show-origin`.
        permissions.config.system = false;
        permissions.config.includes = false;
        permissions.config.git_binary = false;
        permissions.attributes.system = false;
        permissions.attributes.git_binary = false;
        let options = gix::open::Options::default()
            .permissions(permissions)
            .bail_if_untrusted(false)
            .config_overrides([format!("gitoxide.objects.allocLimit={MAX_BLOB}")]);
        crate::pointers::check(path)?;
        let opened = catch_unwind(AssertUnwindSafe(|| options.open(path)));
        let shared = opened
            .map_err(|_| GitProblem::Broken)?
            .map_err(|error| open_problem(&error))?;
        let checked = catch_unwind(AssertUnwindSafe(|| usable(&shared.to_thread_local())));
        checked.map_err(|_| GitProblem::Broken)??;
        Ok(Repo {
            shared,
            path: path.to_path_buf(),
        })
    }

    /// The folder this repository was opened from.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A handle for the current thread.
    #[must_use]
    pub fn local(&self) -> gix::Repository {
        self.shared.to_thread_local()
    }

    /// Resolves `rev` to one commit. A range, a missing name or a non-commit is
    /// `NoSuchRevision`; damaged data, an I/O error or a parser panic is `Broken`.
    pub fn resolve(&self, rev: &str) -> Result<Commit, GitProblem> {
        let repo = self.local();
        let spec = format!("{rev}^{{commit}}");
        let parsed = catch_unwind(AssertUnwindSafe(|| {
            repo.rev_parse(spec.as_str())
                .map(|found| found.single().map(gix::Id::detach))
        }));
        let id = match parsed.map_err(|_| GitProblem::Broken)? {
            Ok(Some(id)) => id,
            Ok(None) => return Err(GitProblem::NoSuchRevision),
            Err(error) => return Err(revision_problem(&error)),
        };
        catch_unwind(AssertUnwindSafe(|| commit_of(&repo, id))).map_err(|_| GitProblem::Broken)?
    }
}

/// Checks an opened repository: SHA-1 objects and a `HEAD` that parses, as `git` requires.
/// Fails with `Unsupported` or `Broken`.
fn usable(repo: &gix::Repository) -> Result<(), GitProblem> {
    if repo.object_hash() != gix::hash::Kind::Sha1 {
        return Err(GitProblem::Unsupported);
    }
    repo.head().map(|_| ()).map_err(|_| GitProblem::Broken)
}

/// Reads the commit `id` and its tree id. Fails with `Broken` when the object cannot be decoded.
fn commit_of(repo: &gix::Repository, id: gix::ObjectId) -> Result<Commit, GitProblem> {
    let commit = repo.find_commit(id).map_err(|_| GitProblem::Broken)?;
    let tree = commit.tree_id().map_err(|_| GitProblem::Broken)?.detach();
    let engine_id = ObjectId::from_bytes(id.as_slice()).ok_or(GitProblem::Broken)?;
    Ok(Commit {
        id: engine_id,
        tree,
        short: id.to_hex_with_len(7).to_string(),
    })
}

/// Maps an open error: absence is `NotRepository`, an unknown format `Unsupported`, the rest
/// (permissions, I/O, damage) `Broken`.
fn open_problem(error: &gix::Error) -> GitProblem {
    match error.dominant_class() {
        Some(Class::NotFound) => GitProblem::NotRepository,
        Some(Class::Unsupported) => GitProblem::Unsupported,
        _ => GitProblem::Broken,
    }
}

/// Maps a `rev_parse` error: a bad or missing name is `NoSuchRevision`; an I/O error or
/// damaged data is `Broken`, so a read failure never looks like a typo.
fn revision_problem(error: &gix::Error) -> GitProblem {
    if error.downcast_any_ref::<std::io::Error>().is_some() {
        return GitProblem::Broken;
    }
    match error.dominant_class() {
        Some(Class::NotFound | Class::Validation | Class::Unsupported) => {
            GitProblem::NoSuchRevision
        }
        _ => GitProblem::Broken,
    }
}

/// Opens the repository at `repo` and resolves `rev` in it. Blocking: call off the UI thread.
pub fn probe(repo: &Path, rev: &str) -> Result<Commit, GitProblem> {
    Repo::open(repo)?.resolve(rev)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Fixture;

    /// A fixture with two commits on `main` and a tag `v1` on the first one.
    fn two_commits() -> (Fixture, gix::ObjectId, gix::ObjectId) {
        let fixture = Fixture::new();
        let first = fixture.commit("main", &[("a.txt", b"one\n")]);
        let second = fixture.commit("main", &[("a.txt", b"two\n"), ("b.txt", b"b\n")]);
        fixture.tag("v1", first);
        (fixture, first, second)
    }

    fn resolve(fixture: &Fixture, rev: &str) -> Result<Commit, GitProblem> {
        Repo::open_isolated(fixture.path())
            .expect("open fixture")
            .resolve(rev)
    }

    #[test]
    fn a_branch_and_its_full_ref_resolve_to_the_tip() {
        let (fixture, _, second) = two_commits();
        let commit = resolve(&fixture, "main").expect("main");
        assert_eq!(commit.id.as_bytes(), second.as_slice());
        assert!(matches!(commit.id, ObjectId::Sha1(_)));
        assert_eq!(commit.short, second.to_hex_with_len(7).to_string());
        assert_eq!(resolve(&fixture, "refs/heads/main"), Ok(commit));
    }

    #[test]
    fn the_commit_carries_its_root_tree() {
        let (fixture, _, second) = two_commits();
        let repo = Repo::open_isolated(fixture.path()).expect("open");
        let tree = repo
            .local()
            .find_commit(second)
            .expect("commit")
            .tree_id()
            .expect("tree")
            .detach();
        assert_eq!(repo.resolve("main").map(|commit| commit.tree), Ok(tree));
    }

    #[test]
    fn an_annotated_tag_and_a_short_prefix_resolve() {
        let (fixture, first, second) = two_commits();
        let tag = resolve(&fixture, "v1").expect("tag");
        assert_eq!(tag.id.as_bytes(), first.as_slice());
        let prefix = second.to_hex_with_len(7).to_string();
        let short = resolve(&fixture, &prefix).expect("prefix");
        assert_eq!(short.id.as_bytes(), second.as_slice());
    }

    #[test]
    fn head_tilde_one_is_the_parent() {
        let (fixture, first, _) = two_commits();
        let parent = resolve(&fixture, "HEAD~1").expect("HEAD~1");
        assert_eq!(parent.id.as_bytes(), first.as_slice());
    }

    #[test]
    fn an_unknown_name_is_no_such_revision() {
        let (fixture, _, _) = two_commits();
        assert_eq!(resolve(&fixture, "nope"), Err(GitProblem::NoSuchRevision));
    }

    #[test]
    fn a_range_is_no_such_revision() {
        let (fixture, _, _) = two_commits();
        assert_eq!(
            resolve(&fixture, "main..main"),
            Err(GitProblem::NoSuchRevision)
        );
    }

    #[test]
    fn a_blob_id_is_no_such_revision() {
        let (fixture, _, _) = two_commits();
        let repo = Repo::open_isolated(fixture.path()).expect("open");
        let blob = repo
            .local()
            .rev_parse_single("main:a.txt")
            .expect("blob")
            .to_string();
        assert_eq!(repo.resolve(&blob), Err(GitProblem::NoSuchRevision));
    }

    #[test]
    fn an_empty_folder_is_not_a_repository() {
        let dir = tempfile::tempdir().expect("temp");
        assert_eq!(
            Repo::open_isolated(dir.path()).map(|_| ()),
            Err(GitProblem::NotRepository)
        );
    }

    #[test]
    fn a_folder_inside_a_work_tree_is_not_a_repository() {
        let (fixture, _, _) = two_commits();
        let inner = fixture.path().join("src");
        std::fs::create_dir(&inner).expect("subfolder");
        assert_eq!(
            Repo::open_isolated(&inner).map(|_| ()),
            Err(GitProblem::NotRepository)
        );
    }

    #[test]
    fn a_garbage_head_fails_without_a_panic() {
        let (fixture, _, _) = two_commits();
        std::fs::write(fixture.path().join(".git/HEAD"), b"garbage\n").expect("write HEAD");
        let opened = std::panic::catch_unwind(|| probe(fixture.path(), "main").map(|_| ()));
        let result = opened.expect("probe must not panic");
        assert!(
            matches!(result, Err(GitProblem::Broken | GitProblem::NotRepository)),
            "a damaged HEAD must not open: {result:?}"
        );
    }

    #[test]
    fn a_sha256_repository_is_unsupported() {
        let (fixture, _, _) = two_commits();
        let config = fixture.path().join(".git/config");
        let text = "[core]\n\trepositoryformatversion = 1\n\tbare = false\n\
                    [extensions]\n\tobjectformat = sha256\n";
        std::fs::write(config, text).expect("write config");
        assert_eq!(
            Repo::open_isolated(fixture.path()).map(|_| ()),
            Err(GitProblem::Unsupported)
        );
    }

    #[test]
    fn opening_never_asks_the_git_program_for_config() {
        let (fixture, _, _) = two_commits();
        let repo = Repo::open(fixture.path()).expect("open");
        let config = repo.local().open_options().permissions.config;
        assert!(!config.git_binary, "the installation config needs `git`");
        assert!(
            !config.system,
            "on Windows the system config path needs `git`"
        );
    }

    #[test]
    fn an_include_in_the_repository_config_is_not_followed() {
        let (fixture, _, _) = two_commits();
        let included = fixture.path().join("included.cfg");
        std::fs::write(&included, "[sidewise]\n\tprobe = yes\n").expect("write include");
        let config = fixture.path().join(".git/config");
        let mut text = std::fs::read_to_string(&config).expect("read config");
        let target = included.display().to_string().replace('\\', "/");
        text.push_str(&format!(
            "[include]\n\tpath = {target}\n[includeIf \"onbranch:main\"]\n\tpath = {target}\n"
        ));
        std::fs::write(&config, text).expect("write config");
        let repo = Repo::open(fixture.path()).expect("open");
        let value = repo.local().config_snapshot().string("sidewise.probe");
        assert_eq!(value, None, "an include can name an SMB share");
    }

    #[test]
    fn one_repository_handle_is_shared_between_threads() {
        fn shared<T: Send + Sync>() {}
        shared::<Repo>();
    }

    #[test]
    fn probe_of_a_missing_folder_is_not_a_repository() {
        let dir = tempfile::tempdir().expect("temp");
        let missing = dir.path().join("missing");
        assert_eq!(
            probe(&missing, "main").map(|_| ()),
            Err(GitProblem::NotRepository)
        );
    }
}
