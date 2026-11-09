use std::ffi::OsString;
use std::time::Duration;

use crate::GitError;

use super::parse::protocol;
use super::process::CommandSpec;
use super::process_cap::run_cancellable;
use super::remote::PROMPT_OFF;
use super::sync::{SyncTarget, base_args};
use super::sync_read::{needs_credentials, refuses_fast_forward, would_overwrite};
use super::{Cancel, CliGit, ProcessOptions, ProcessOutcome, ProcessOutput};

const SYNC_STDOUT_CAP: usize = 4 * 1024 * 1024;

/// How a started Pull or Push ended: Git exited by itself, or Navo stopped it with `Killed`'s
/// reason. An exit 0 always counts as `Exited`, even when a cancel or deadline came after it.
pub(super) enum Ran {
    Exited(ProcessOutput),
    Killed(GitError),
}

/// The addresses and fetch refspecs of the upstream remote, as Git resolves them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemoteBinding {
    pub urls: Vec<String>,
    pub push_urls: Vec<String>,
    pub fetch: Vec<String>,
}

/// Refuses a Pull unless the remote fetches only its branches into `refs/remotes/<remote>/`;
/// any other refspec defeats `--no-tags` and may write tags or local branches.
pub(super) fn require_default_fetch(binding: &RemoteBinding, remote: &str) -> Result<(), GitError> {
    let default = format!("+refs/heads/*:refs/remotes/{remote}/*");
    match binding.fetch.as_slice() {
        [only] if *only == default => Ok(()),
        _ => Err(GitError::UnsupportedRefspec),
    }
}

impl CliGit {
    /// Refuses when the config, the branch, its upstream, the remote or HEAD differ from the
    /// confirmation.
    pub(super) fn confirmed(&self, target: &SyncTarget) -> Result<(), GitError> {
        if self.guard()? != target.guard {
            return Err(GitError::GuardChanged);
        }
        if self.tracking()? != target.tracking || self.commit_of("HEAD")? != target.head {
            return Err(GitError::TargetChanged);
        }
        if self.remote_binding(&target.tracking.remote)? != target.remote {
            return Err(GitError::GuardChanged);
        }
        Ok(())
    }

    pub(super) fn remote_binding(&self, remote: &str) -> Result<RemoteBinding, GitError> {
        let fetch_key = format!("remote.{remote}.fetch");
        Ok(RemoteBinding {
            urls: self.lines_of(&["remote", "get-url", "--all", remote])?,
            push_urls: self.lines_of(&["remote", "get-url", "--push", "--all", remote])?,
            fetch: self.config_lines(&fetch_key)?,
        })
    }

    /// Refuses a Push whose upstream branch left a push address while its tracking ref stayed:
    /// the push would recreate a branch deleted after a merge. Every confirmed push address
    /// must list the branch. A branch deleted between this check and the push is still
    /// recreated; only a forced lease could stop that, and Navo never forces.
    pub(super) fn require_remote_branch(
        &self,
        target: &SyncTarget,
        cancel: &Cancel,
        timeout: Duration,
    ) -> Result<(), GitError> {
        let tracking = &target.tracking;
        if self.commit_of(&tracking.tracking_ref)?.is_none() {
            return Ok(());
        }
        let branch = format!("refs/heads/{}", tracking.remote_branch);
        let mut probed: Vec<&str> = Vec::new();
        for url in &target.remote.push_urls {
            if !probed.contains(&url.as_str()) {
                self.require_listed(url, &branch, (cancel, timeout))?;
                probed.push(url);
            }
        }
        if probed.is_empty() {
            self.require_listed(&tracking.remote, &branch, (cancel, timeout))?;
        }
        Ok(())
    }

    /// `ls-remote` of one push address; fails unless it lists exactly `branch`.
    fn require_listed(
        &self,
        address: &str,
        branch: &str,
        (cancel, timeout): (&Cancel, Duration),
    ) -> Result<(), GitError> {
        let mut args = base_args();
        args.extend(["ls-remote", "--exit-code", "--end-of-options"].map(OsString::from));
        args.extend([address, branch].map(OsString::from));
        let output = match self.sync_run(&args, cancel, timeout)? {
            Ran::Exited(output) => output,
            Ran::Killed(error) => return Err(error),
        };
        let code = output.status.and_then(|status| status.code());
        match code {
            Some(0) if lists_ref(&output.stdout, branch) => return Ok(()),
            Some(0 | 2) => return Err(GitError::RemoteBranchGone),
            _ => {}
        }
        if needs_credentials(&output.stderr) {
            return Err(GitError::AuthFailed);
        }
        Err(self.failed(&args, &output))
    }

    fn lines_of(&self, args: &[&str]) -> Result<Vec<String>, GitError> {
        let output = self.run_ok(args)?;
        let text = std::str::from_utf8(&output.stdout).map_err(|_| protocol())?;
        Ok(text.lines().map(str::to_owned).collect())
    }

    /// Every value of `key`; empty when it is unset.
    fn config_lines(&self, key: &str) -> Result<Vec<String>, GitError> {
        let args = ["config", "--get-all", key];
        let output = self.run(&args)?;
        match output.status.code() {
            Some(0) => {
                let text = std::str::from_utf8(&output.stdout).map_err(|_| protocol())?;
                Ok(text.lines().map(str::to_owned).collect())
            }
            Some(1) => Ok(Vec::new()),
            _ => Err(self.command_error(&args, &output)),
        }
    }

    pub(super) fn sync_run(
        &self,
        args: &[OsString],
        cancel: &Cancel,
        timeout: Duration,
    ) -> Result<Ran, GitError> {
        let mut env = self.env.clone();
        env.extend(PROMPT_OFF.map(|(key, value)| (key.into(), value.into())));
        let spec = CommandSpec {
            git: &self.git,
            repo: Some(&self.repo),
            git_dir: None,
            env: &env,
            timeout,
        };
        let options = ProcessOptions {
            stdout_cap: SYNC_STDOUT_CAP,
            timeout,
        };
        let output = run_cancellable(&spec, args, cancel, &options)?;
        let Some(status) = output.status else {
            return match output.outcome {
                ProcessOutcome::Cancelled => Err(GitError::Cancelled),
                _ => Err(GitError::SessionClosed),
            };
        };
        if status.success() || output.outcome == ProcessOutcome::Done {
            return Ok(Ran::Exited(output));
        }
        Ok(Ran::Killed(match output.outcome {
            ProcessOutcome::TimedOut => GitError::Timeout,
            ProcessOutcome::Capped => protocol(),
            _ => GitError::Cancelled,
        }))
    }

    /// After an exit 0: HEAD holds the fetched `upstream`, no stash was added and no conflict
    /// is left.
    pub(super) fn verify_pull(&self, upstream: &str, stash: Option<&str>) -> Result<(), GitError> {
        let reached = self.is_ancestor(upstream, "HEAD")?;
        let stashed = self.commit_of("refs/stash")?.as_deref() != stash;
        let unmerged = !self
            .run_ok(&["ls-files", "--unmerged", "-z"])?
            .stdout
            .is_empty();
        if !reached || stashed || unmerged {
            return Err(GitError::PullUnfinished);
        }
        Ok(())
    }

    /// What a killed Pull left: an `index.lock` and whether HEAD moved from `before`.
    pub(super) fn interrupted(&self, before: Option<&str>) -> GitError {
        let lock = match self.git_dir() {
            Ok(git_dir) => git_dir.join("index.lock").exists(),
            Err(error) => return error,
        };
        match self.commit_of("HEAD") {
            Ok(after) => GitError::Interrupted {
                lock,
                moved: after.as_deref() != before,
            },
            Err(error) => error,
        }
    }

    pub(super) fn pull_failure(
        &self,
        args: &[OsString],
        output: &ProcessOutput,
        upstream: &str,
    ) -> GitError {
        if needs_credentials(&output.stderr) {
            return GitError::AuthFailed;
        }
        match self.is_ancestor("HEAD", upstream).map(|reached| !reached) {
            Ok(true) if refuses_fast_forward(&output.stderr) => GitError::NotFastForward,
            Ok(_) if would_overwrite(&output.stderr) => GitError::LocalChanges,
            Ok(_) => self.failed(args, output),
            Err(error) => error,
        }
    }

    pub(super) fn failed(&self, args: &[OsString], output: &ProcessOutput) -> GitError {
        match output.status {
            Some(status) => self.command_error(
                args,
                &std::process::Output {
                    status,
                    stdout: Vec::new(),
                    stderr: output.stderr.clone(),
                },
            ),
            None => GitError::SessionClosed,
        }
    }

    pub(super) fn is_ancestor(&self, ancestor: &str, of: &str) -> Result<bool, GitError> {
        let args = [
            "merge-base",
            "--is-ancestor",
            "--end-of-options",
            ancestor,
            of,
        ];
        let output = self.run(&args)?;
        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => Err(self.command_error(&args, &output)),
        }
    }
}

/// True when an `ls-remote` listing has a line for exactly `full_ref`.
fn lists_ref(stdout: &[u8], full_ref: &str) -> bool {
    String::from_utf8_lossy(stdout)
        .lines()
        .any(|line| line.split('\t').nth(1) == Some(full_ref))
}
