//! The command-line Git backend: resolves Git once and reads a repository through `CliGit`.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use crate::trust::Guard;
use crate::{
    BlobReader, Change, GitError, GitPath, GitRef, GitSource, IndexEntry, ObjectId, OtherPath,
    Others, Stage, Target, TreeEntry, WorktreeBlob,
};

mod batch;
mod changes;
mod filtered_stream;
mod filters;
mod locate;
pub(crate) mod parse;
mod probe_cache;
mod process;
mod process_cap;
mod process_tree;
mod process_watch;
#[cfg(test)]
mod raw_odb_tests;
mod remote;
mod running;
mod status;
mod sync;
mod sync_check;
mod sync_read;

use batch::{Batch, BlobStream};
use locate::find_git;
use parse::{missing_revision, parse_oid, parse_refs, parse_tree};
use process::{CommandSpec, base_command, command_io, run_process};

pub use process_cap::{ProcessOptions, ProcessOutput};
pub use process_watch::{Cancel, ProcessOutcome};
pub use running::{GitBusy, GitPause, stop_under};
pub use status::StatusLimits;
pub use sync::{SYNC_TIMEOUT, SyncKind, SyncPreview, SyncSummary, SyncTarget};
pub use sync_check::RemoteBinding;
pub use sync_read::Tracking;

pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
pub const MIN_GIT_VERSION: (u32, u32) = (2, 36);

/// How long an unused `cat-file` child may stay alive before the next read replaces it.
const BATCH_IDLE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone)]
pub struct CliOptions {
    pub timeout: Duration,
    pub git: Option<PathBuf>,
    pub env: Vec<(OsString, OsString)>,
    pub raw_odb_only: bool,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            git: None,
            env: Vec::new(),
            raw_odb_only: false,
        }
    }
}

pub struct CliGit {
    git: PathBuf,
    repo: PathBuf,
    timeout: Duration,
    env: Vec<(OsString, OsString)>,
    pub(super) raw_odb_only: bool,
    /// Output of `rev-parse --absolute-git-dir`, read once when the repository is opened.
    git_dir_output: Vec<u8>,
    /// Whether the repo configures a promisor remote, cached on first raw-ODB "bad file" check.
    has_promisor: OnceLock<bool>,
    session: Arc<Mutex<Option<Batch>>>,
    next_generation: AtomicU64,
    /// How long an unused batch child may stay alive before the next read replaces it.
    batch_idle: Duration,
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
        let (requested, resolve) = match &options.git {
            Some(git) => (git.clone(), cfg!(windows)),
            None => (find_git(repo)?, true),
        };
        let (git, version, text) =
            probe_cache::probe(&requested, resolve, &options.env, options.timeout)?;
        if (version.major, version.minor) < MIN_GIT_VERSION {
            return Err(GitError::GitTooOld { version: text });
        }
        let mut cli = Self {
            git,
            repo: repo.to_path_buf(),
            timeout: options.timeout,
            env: options.env.clone(),
            raw_odb_only: options.raw_odb_only,
            git_dir_output: Vec::new(),
            has_promisor: OnceLock::new(),
            session: Arc::new(Mutex::new(None)),
            next_generation: AtomicU64::new(0),
            batch_idle: BATCH_IDLE,
        };
        cli.git_dir_output = cli.run_ok(&["rev-parse", "--absolute-git-dir"])?.stdout;
        Ok(cli)
    }

    /// Captures bounded output and joins every reader before returning.
    pub fn run_cancellable<S: AsRef<OsStr>>(
        &self,
        args: &[S],
        cancel: &Cancel,
        options: &ProcessOptions,
    ) -> Result<ProcessOutput, GitError> {
        let spec = CommandSpec {
            git: &self.git,
            repo: Some(&self.repo),
            git_dir: None,
            env: &self.env,
            timeout: options.timeout,
        };
        if self.raw_odb_only {
            return process_cap::run_cancellable(&spec, &raw_odb_args(args), cancel, options);
        }
        process_cap::run_cancellable(&spec, args, cancel, options)
    }

    /// The D100e guard for background calls on this `CliGit`: Git resolves the includes with the
    /// same cwd and environment, and the reader cross-checks it. Blocked when `env` sets a home.
    pub fn guard(&self) -> Result<Guard, GitError> {
        if self.env.iter().any(|(key, _)| sets_home(key)) {
            return Ok(Guard::unreadable());
        }
        let reader = Guard::read_reported(&self.git_dir_output)?;
        if reader.has_special_file() {
            return Ok(reader);
        }
        match self.run_ok(&["config", "--list", "-z", "--show-scope", "--includes"]) {
            Ok(listed) => Ok(reader.cross_check(&listed.stdout)),
            Err(error @ GitError::UnsafeDirectory { .. }) => Err(error),
            Err(_) => Ok(Guard::unreadable()),
        }
    }

    fn run<S: AsRef<OsStr>>(&self, args: &[S]) -> Result<Output, GitError> {
        let spec = CommandSpec {
            git: &self.git,
            repo: Some(&self.repo),
            git_dir: None,
            env: &self.env,
            timeout: self.timeout,
        };
        if self.raw_odb_only {
            return run_process(&spec, &raw_odb_args(args));
        }
        run_process(&spec, args)
    }

    fn run_ok<S: AsRef<OsStr>>(&self, args: &[S]) -> Result<Output, GitError> {
        let output = self.run(args)?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(self.command_error(args, &output))
        }
    }

    fn command_error<S: AsRef<OsStr>>(&self, args: &[S], output: &Output) -> GitError {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if stderr.contains("detected dubious ownership") {
            return GitError::UnsafeDirectory {
                path: self.repo.clone(),
            };
        }
        if self.is_missing_promisor(&stderr) {
            return GitError::MissingPromisorBlob;
        }
        let args = args
            .iter()
            .map(|arg| arg.as_ref().to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        GitError::Command {
            args,
            status: output.status.code(),
            stderr,
        }
    }

    /// Builds `cat-file` against the absolute git dir from the temp folder, never the work tree.
    fn batch_command(&self) -> Result<Command, GitError> {
        let git_dir = self.git_dir()?;
        let spec = CommandSpec {
            git: &self.git,
            repo: None,
            git_dir: Some(&git_dir),
            env: &self.env,
            timeout: self.timeout,
        };
        let mut command = base_command(&spec);
        if self.raw_odb_only {
            command.args(["-c", "core.attributesfile="]);
        }
        Ok(command)
    }

    /// Starts the long-lived `cat-file` batch child.
    fn spawn_batch(&self) -> Result<Batch, GitError> {
        let mut command = self.batch_command()?;
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

    /// Classifies `stderr` as a missing promisor blob, consulting repo config at most once.
    fn is_missing_promisor(&self, stderr: &str) -> bool {
        is_explicit_promisor_stderr(stderr)
            || (self.raw_odb_only && is_raw_bad_file_stderr(stderr) && self.has_promisor_remote())
    }

    /// Returns whether the repository configures a promisor remote, cached per `CliGit` handle.
    fn has_promisor_remote(&self) -> bool {
        *self
            .has_promisor
            .get_or_init(|| self.read_has_promisor_remote())
    }

    fn read_has_promisor_remote(&self) -> bool {
        let Ok(output) = self.run(&["config", "--list", "-z", "--includes"]) else {
            return false;
        };
        output.status.success() && parse_has_promisor(&output.stdout)
    }
}

/// `~/` includes resolve against Navo's own home, so the child must inherit it unchanged.
fn sets_home(key: &OsStr) -> bool {
    ["HOME", "USERPROFILE"]
        .iter()
        .any(|home| key.eq_ignore_ascii_case(home))
}

/// Prepends `-c core.attributesfile=` so raw ODB commands ignore external attributes files.
fn raw_odb_args<S: AsRef<OsStr>>(args: &[S]) -> Vec<&OsStr> {
    [OsStr::new("-c"), OsStr::new("core.attributesfile=")]
        .into_iter()
        .chain(args.iter().map(AsRef::as_ref))
        .collect()
}

/// Recognizes missing-promisor stderr lines from `git cat-file` when lazy fetch is disabled.
#[cfg(test)]
pub(super) fn is_missing_promisor_stderr(
    stderr: &str,
    raw_odb_only: bool,
    has_promisor_remote: bool,
) -> bool {
    is_explicit_promisor_stderr(stderr)
        || (raw_odb_only && has_promisor_remote && is_raw_bad_file_stderr(stderr))
}

fn is_explicit_promisor_stderr(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("from promisor remote")
        || lower.contains("promisor blob")
        || lower.contains("missing promisor")
        || lower.contains("lazy fetching disabled")
        || (lower.contains("promisor") && lower.contains("missing"))
}

fn is_raw_bad_file_stderr(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("fatal: git cat-file ") && lower.contains(": bad file")
}

/// Parses `git config --list -z` output for `remote.<name>.promisor=true` or `extensions.partialClone`.
pub(super) fn parse_has_promisor(listed: &[u8]) -> bool {
    let mut promisors: HashMap<&[u8], bool> = HashMap::new();
    let mut partial_clone: Option<&[u8]> = None;
    for entry in listed.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        let mut parts = entry.splitn(2, |byte| *byte == b'\n');
        let Some(key) = parts.next() else {
            continue;
        };
        let value = parts.next();
        if key == b"extensions.partialclone" {
            partial_clone = value;
        } else if let Some(remote) = key
            .strip_prefix(b"remote.")
            .and_then(|rest| rest.strip_suffix(b".promisor"))
            && !remote.is_empty()
        {
            promisors.insert(remote, is_git_true(value));
        }
    }
    partial_clone.is_some_and(|value| !value.is_empty())
        || promisors.values().any(|&enabled| enabled)
}

fn is_git_true(value: Option<&[u8]>) -> bool {
    match value {
        None => true,
        Some(raw) => {
            raw.eq_ignore_ascii_case(b"true")
                || raw.eq_ignore_ascii_case(b"yes")
                || raw.eq_ignore_ascii_case(b"on")
                || raw == b"1"
        }
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

    /// Starts a blob read, replacing a dead or idle batch child first.
    fn read_blob(&self, id: &ObjectId) -> Result<BlobReader, GitError> {
        let mut session = self.session.lock().map_err(|_| GitError::SessionClosed)?;
        let stale = session
            .as_ref()
            .is_some_and(|batch| batch.is_dead() || batch.is_idle(self.batch_idle));
        if stale {
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
                drop(session);
                if matches!(error, GitError::Command { ref stderr, .. } if self.is_missing_promisor(stderr))
                {
                    Err(GitError::MissingPromisorBlob)
                } else {
                    Err(error)
                }
            }
        }
    }

    fn read_blob_worktree_form(&self, rev: &str, path: &GitPath) -> Result<WorktreeBlob, GitError> {
        filters::read_worktree_form(self, rev, path)
    }

    fn changes(&self, a: &str, target: Target<'_>) -> Result<Vec<Change>, GitError> {
        changes::read_changes(self, a, target)
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

    fn index_entries(&self, stage: Stage) -> Result<Vec<IndexEntry>, GitError> {
        changes::read_index_entries(self, stage)
    }

    fn others(&self, which: Others) -> Result<Vec<OtherPath>, GitError> {
        filters::read_others(self, which)
    }
}
