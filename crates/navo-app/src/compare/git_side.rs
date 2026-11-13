//! A Git revision as a read-only compare side.

use std::io::Read;
use std::ops::ControlFlow;
use std::path::PathBuf;

use navo_compare::model::{
    CancelToken, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use navo_core::{FileIdentity, Location, ProviderId, ProviderPath};
use navo_fs::git::{GitFs, location, provider_id};
use navo_fs::{CancelToken as FsCancel, Provider};
use navo_git::{CliGit, CliOptions, EngineKind, GixGit};

use super::entry::{feed, reason_of, segments};

/// The files of `repo@rev`. Submodules list as plain folders and cannot be entered.
pub struct GitSide {
    git: GitFs,
    repo: PathBuf,
    rev: String,
    provider: ProviderId,
}

impl GitSide {
    /// Opens a side with the given engine; fails with a reason when the repository or the
    /// revision is missing.
    pub fn new(repo: PathBuf, rev: String, engine: EngineKind) -> Result<Self, Reason> {
        Self::open_with(git_fs(engine), repo, rev)
    }

    /// Opens a CLI Git side reading raw ODB objects without filters or attributes.
    pub fn new_raw_odb(repo: PathBuf, rev: String) -> Result<Self, Reason> {
        let git = GitFs::with_factory(|repo| {
            Ok(Box::new(CliGit::with_options(
                repo,
                &CliOptions {
                    raw_odb_only: true,
                    ..CliOptions::default()
                },
            )?))
        });
        Self::open_with(git, repo, rev)
    }

    fn open_with(git: GitFs, repo: PathBuf, rev: String) -> Result<Self, Reason> {
        let side = Self {
            git,
            repo,
            rev,
            provider: provider_id(),
        };
        let root = side.location(&RelPath::root())?;
        side.git
            .metadata(&root)
            .map_err(|error| reason_of(error.kind))?;
        Ok(side)
    }

    /// Forgets the resolved revision, so the next listing sees the current commit.
    pub fn refresh(&self) {
        self.git.refresh();
    }

    /// Resolves the Git object ID for `rel`, or `None` when missing or unresolvable.
    pub fn object_id(&self, rel: &RelPath) -> Option<ObjectId> {
        self.git.object_id(&self.location(rel).ok()?).ok()
    }

    fn location(&self, rel: &RelPath) -> Result<Location, Reason> {
        let inner = segments(rel).join("/");
        location(&self.repo, &self.rev, &inner).map_err(|_| Reason::Unsupported)
    }
}

fn git_fs(engine: EngineKind) -> GitFs {
    match engine {
        EngineKind::Gix => GitFs::with_factory(|repo| Ok(Box::new(GixGit::new(repo)?))),
        EngineKind::Cli => GitFs::new(),
    }
}

impl SideSource for GitSide {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn list(
        &self,
        folder: &RelPath,
        sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        cancel: &CancelToken,
    ) -> Result<(), Reason> {
        let at = self.location(folder)?;
        let mut batch = |batch| feed(folder, batch, sink, cancel);
        self.git
            .list(&at, &mut batch, &FsCancel::new())
            .map_err(|error| reason_of(error.kind))
    }

    fn open(&self, rel: &RelPath, _hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        let at = self.location(rel)?;
        self.git.read(&at).map_err(|error| reason_of(error.kind))
    }

    /// Returns known metadata for `rel`; probes unknown blobs to preserve their read failure.
    fn stamp(&self, rel: &RelPath) -> Result<Stamp, Reason> {
        let at = self.location(rel)?;
        let found = self
            .git
            .metadata(&at)
            .map_err(|error| reason_of(error.kind))?;
        if found.kind == navo_fs::EntryKind::File && found.size == 0 {
            let entry = self
                .git
                .tree_entry(&at)
                .map_err(|error| reason_of(error.kind))?;
            if entry.size.is_none() {
                self.open(rel, ReadHint::Buffered)?;
                return Err(Reason::ChangedOnDisk);
            }
        }
        Ok(Stamp {
            size: found.size,
            modified: None,
            file_id: None,
        })
    }

    fn object_id(&self, rel: &RelPath) -> Option<ObjectId> {
        GitSide::object_id(self, rel)
    }

    fn file_id(&self, _rel: &RelPath) -> Option<FileIdentity> {
        None
    }

    fn cache_path(&self, rel: &RelPath) -> ProviderPath {
        self.location(rel)
            .map_or_else(|_| rel.as_path().clone(), |at| at.path().clone())
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        true
    }

    fn may_have_unknown_size(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use navo_compare::model::ReadHint;
    use navo_git::GitError;

    use super::*;

    #[test]
    fn a_paused_read_shows_locked_not_a_read_error() {
        let side = GitSide {
            git: GitFs::with_factory(|_| Err(GitError::Paused)),
            repo: PathBuf::from("repo"),
            rev: "HEAD".to_owned(),
            provider: provider_id(),
        };
        let error = side
            .open(&RelPath::root(), ReadHint::Buffered)
            .err()
            .expect("a paused read must fail");
        assert_eq!(error, Reason::Locked);
    }

    #[test]
    fn missing_promisor_blob_maps_to_missing_blob() {
        assert_eq!(
            reason_of(navo_core::FsErrorKind::MissingPromisorBlob),
            Reason::MissingBlob
        );
    }

    #[test]
    fn object_id_returns_blob_id_from_raw_odb() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let run = |args: &[&str]| {
            std::process::Command::new("git")
                .current_dir(repo)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .args(["-c", "user.name=t", "-c", "user.email=t@example.invalid"])
                .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
                .args(args)
                .output()
                .unwrap()
        };
        assert!(run(&["init", "-q"]).status.success());
        std::fs::write(repo.join("a.txt"), b"alpha").unwrap();
        assert!(run(&["add", "."]).status.success());
        assert!(run(&["commit", "-q", "-m", "init"]).status.success());
        let expected = String::from_utf8(run(&["rev-parse", "HEAD:a.txt"]).stdout)
            .unwrap()
            .trim()
            .to_owned();

        let side = GitSide::new_raw_odb(repo.to_path_buf(), "HEAD".to_owned()).unwrap();
        let id = GitSide::object_id(&side, &RelPath::new(ProviderPath::from_text("a.txt")))
            .expect("tracked blob must have an object id");
        let actual: String = id.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(actual, expected);
        assert!(
            GitSide::object_id(&side, &RelPath::new(ProviderPath::from_text("missing.txt")))
                .is_none()
        );
    }
}
