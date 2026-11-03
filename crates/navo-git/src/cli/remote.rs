use std::ffi::OsString;
use std::time::Duration;

use crate::GitError;
use crate::trust::{Guard, NO_AUTO_MAINTENANCE};

use super::parse::protocol;
use super::process::CommandSpec;
use super::process_cap::run_cancellable;
use super::{Cancel, CliGit, ProcessOptions};

/// The deadline of a background fetch (D100g).
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(60);

const FETCH_STDOUT_CAP: usize = 64 * 1024;

pub(super) const PROMPT_OFF: [(&str, &str); 3] = [
    ("GCM_INTERACTIVE", "never"),
    ("SSH_ASKPASS_REQUIRE", "never"),
    ("GIT_ASKPASS", ""),
];

impl CliGit {
    /// `git fetch --quiet --no-recurse-submodules --no-prune <upstream remote>` with no prompt and no hook.
    /// Refuses a blocked or guarded `guard`, a detached HEAD, no upstream and a remote that starts
    /// with `-`; none of these start a fetch. Never prunes, never fetches tags by force.
    pub fn fetch(&self, guard: &Guard, cancel: &Cancel) -> Result<(), GitError> {
        self.fetch_with(guard, cancel, FETCH_TIMEOUT)
    }

    /// `fetch` with an explicit deadline.
    pub fn fetch_with(
        &self,
        guard: &Guard,
        cancel: &Cancel,
        timeout: Duration,
    ) -> Result<(), GitError> {
        if guard.is_blocked() {
            return Err(GitError::NotTrusted {
                path: self.repo.clone(),
            });
        }
        if guard.is_guarded() {
            return Err(GitError::Guarded {
                path: self.repo.clone(),
            });
        }
        let remote = self.upstream_remote()?;
        let mut args = guard.fetch_args();
        args.extend(["-c", "credential.interactive=never"].map(OsString::from));
        args.extend(
            ["fetch", "--quiet", "--no-recurse-submodules", "--no-prune"].map(OsString::from),
        );
        args.extend([NO_AUTO_MAINTENANCE, &remote].map(OsString::from));
        let mut env = self.env.clone();
        env.extend(PROMPT_OFF.map(|(key, value)| (key.into(), value.into())));
        let spec = CommandSpec {
            git: &self.git,
            repo: Some(&self.repo),
            env: &env,
            timeout,
        };
        let options = ProcessOptions {
            stdout_cap: FETCH_STDOUT_CAP,
            timeout,
        };
        let output = run_cancellable(&spec, &args, cancel, &options)?;
        if self.read_outcome(&args, output.outcome, &output)? {
            return Err(protocol());
        }
        Ok(())
    }

    /// The remote of the current branch's upstream, as Git configures it.
    fn upstream_remote(&self) -> Result<String, GitError> {
        let branch = self.current_branch()?;
        let args = [
            "for-each-ref",
            "--format=%(upstream:remotename)%00%(upstream)",
            "--end-of-options",
            &branch,
        ];
        let listed = self.run_ok(&args)?;
        let text = std::str::from_utf8(&listed.stdout).map_err(|_| protocol())?;
        let remote = match text.trim_end().split_once('\0') {
            Some((remote, upstream)) if !remote.is_empty() && !upstream.is_empty() => remote,
            _ => return Err(GitError::NoUpstream),
        };
        if remote.starts_with('-') {
            return Err(GitError::UnsafeRemote {
                name: remote.to_owned(),
            });
        }
        Ok(remote.to_owned())
    }

    /// The full ref of the checked-out branch; `DetachedHead` when there is none.
    pub(super) fn current_branch(&self) -> Result<String, GitError> {
        let head = self.run(&["symbolic-ref", "-q", "HEAD"])?;
        match head.status.code() {
            Some(0) => {}
            Some(1) => return Err(GitError::DetachedHead),
            _ => return Err(self.command_error(&["symbolic-ref", "-q", "HEAD"], &head)),
        }
        let branch = std::str::from_utf8(&head.stdout).map_err(|_| protocol())?;
        Ok(branch.trim_end().to_owned())
    }
}
