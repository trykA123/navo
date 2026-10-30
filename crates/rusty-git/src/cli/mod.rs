use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::{
    BlobReader, Change, GitError, GitPath, GitRef, GitSource, IndexEntry, ObjectId, OtherPath,
    Others, Stage, Target, TreeEntry,
};

mod batch;
mod parse;
mod process;

use batch::{Batch, BlobStream};
use parse::{missing_revision, parse_oid, parse_refs, parse_tree};
use process::{
    CommandSpec, base_command, command_io, find_git, resolve_git, run_process, version_of,
};

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
pub const MIN_GIT_VERSION: (u32, u32) = (2, 36);

#[derive(Debug, Clone)]
pub struct CliOptions {
    pub timeout: Duration,
    pub git: Option<PathBuf>,
    pub env: Vec<(OsString, OsString)>,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            git: None,
            env: Vec::new(),
        }
    }
}

pub struct CliGit {
    git: PathBuf,
    repo: PathBuf,
    timeout: Duration,
    env: Vec<(OsString, OsString)>,
    session: Arc<Mutex<Option<Batch>>>,
    next_generation: AtomicU64,
}

impl CliGit {
    pub fn new(repo: &Path) -> Result<Self, GitError> {
        Self::with_options(repo, &CliOptions::default())
    }

    pub fn with_timeout(repo: &Path, timeout: Duration) -> Result<Self, GitError> {
        Self::with_options(
            repo,
            &CliOptions {
                timeout,
                ..CliOptions::default()
            },
        )
    }

    pub fn with_options(repo: &Path, options: &CliOptions) -> Result<Self, GitError> {
        let git = match &options.git {
            Some(git) => git.clone(),
            None => resolve_git(&find_git()?, &options.env, options.timeout),
        };
        let (version, text) = version_of(&git, &options.env, options.timeout)?;
        if (version.major, version.minor) < MIN_GIT_VERSION {
            return Err(GitError::GitTooOld { version: text });
        }
        let cli = Self {
            git,
            repo: repo.to_path_buf(),
            timeout: options.timeout,
            env: options.env.clone(),
            session: Arc::new(Mutex::new(None)),
            next_generation: AtomicU64::new(0),
        };
        cli.run_ok(&["rev-parse", "--git-dir"])?;
        Ok(cli)
    }

    fn run(&self, args: &[&str]) -> Result<Output, GitError> {
        let spec = CommandSpec {
            git: &self.git,
            repo: Some(&self.repo),
            env: &self.env,
            timeout: self.timeout,
        };
        run_process(&spec, args)
    }

    fn run_ok(&self, args: &[&str]) -> Result<Output, GitError> {
        let output = self.run(args)?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(self.command_error(args, &output))
        }
    }

    fn command_error(&self, args: &[&str], output: &Output) -> GitError {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if stderr.contains("detected dubious ownership") {
            return GitError::UnsafeDirectory {
                path: self.repo.clone(),
            };
        }
        GitError::Command {
            args: args.join(" "),
            status: output.status.code(),
            stderr,
        }
    }

    fn spawn_batch(&self) -> Result<Batch, GitError> {
        let spec = CommandSpec {
            git: &self.git,
            repo: Some(&self.repo),
            env: &self.env,
            timeout: self.timeout,
        };
        let mut command = base_command(&spec);
        command
            .args(["cat-file", "--batch-command", "--buffer"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = command
            .spawn()
            .map_err(|error| command_io(&self.git, error))?;
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        Batch::new(child, self.repo.clone(), generation)
    }
}

impl GitSource for CliGit {
    fn resolve(&self, name: &str) -> Result<Option<ObjectId>, GitError> {
        let revision = format!("{name}^{{commit}}");
        let args = ["rev-parse", "--verify", "--end-of-options", &revision];
        let output = self.run(&args)?;
        if output.status.success() {
            return parse_oid(&output.stdout).map(Some);
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        if missing_revision(&stderr) {
            return Ok(None);
        }
        Err(self.command_error(&args, &output))
    }

    fn refs(&self) -> Result<Vec<GitRef>, GitError> {
        let output = self.run_ok(&[
            "for-each-ref",
            "--sort=refname",
            "--format=%(refname)%00%(objectname)",
            "--end-of-options",
        ])?;
        parse_refs(&output.stdout)
    }

    fn list_tree(&self, rev: &str) -> Result<Vec<TreeEntry>, GitError> {
        let output = self.run_ok(&[
            "ls-tree",
            "-r",
            "-z",
            "-l",
            "--full-tree",
            "--end-of-options",
            rev,
        ])?;
        parse_tree(&output.stdout)
    }

    fn read_blob(&self, id: &ObjectId) -> Result<BlobReader, GitError> {
        let mut session = self.session.lock().map_err(|_| GitError::SessionClosed)?;
        if session.as_ref().is_some_and(Batch::is_dead) {
            *session = None;
        }
        if session.is_none() {
            *session = Some(self.spawn_batch()?);
        }
        let started = match session.as_mut() {
            Some(batch) => batch.start_blob(id, self.timeout),
            None => Err(GitError::SessionClosed),
        };
        match started {
            Ok(started) => {
                drop(session);
                Ok(Box::new(BlobStream::new(
                    Arc::clone(&self.session),
                    started,
                )))
            }
            Err(error) => {
                if session.as_ref().is_some_and(Batch::is_dead) {
                    *session = None;
                }
                Err(error)
            }
        }
    }

    fn read_blob_worktree_form(&self, _rev: &str, _path: &GitPath) -> Result<BlobReader, GitError> {
        Err(unsupported("read_blob_worktree_form"))
    }

    fn changes(&self, _a: &str, _target: Target<'_>) -> Result<Vec<Change>, GitError> {
        Err(unsupported("changes"))
    }

    fn merge_base(&self, a: &str, b: &str) -> Result<Option<ObjectId>, GitError> {
        let args = ["merge-base", "--end-of-options", a, b];
        let output = self.run(&args)?;
        if output.status.success() {
            return parse_oid(&output.stdout).map(Some);
        }
        if output.status.code() == Some(1) {
            return Ok(None);
        }
        Err(self.command_error(&args, &output))
    }

    fn index_entries(&self, _stage: Stage) -> Result<Vec<IndexEntry>, GitError> {
        Err(unsupported("index_entries"))
    }

    fn others(&self, _which: Others) -> Result<Vec<OtherPath>, GitError> {
        Err(unsupported("others"))
    }
}

fn unsupported(operation: &str) -> GitError {
    GitError::Unsupported {
        operation: operation.to_owned(),
    }
}
