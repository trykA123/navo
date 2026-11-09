//! Builds and runs bounded Git subprocesses for the command-line backend.

#[cfg(test)]
use std::cell::RefCell;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::GitError;

use super::parse::protocol;
use super::process_tree::GitTree;
use super::process_watch::Cancel;
use super::running::{self, Ticket, Work};

#[cfg(test)]
type AfterReapHook = Box<dyn FnOnce(Option<&Cancel>)>;

#[cfg(test)]
thread_local! {
    /// A one-shot hook that runs after `wait` observes the exit.
    static AFTER_REAP: RefCell<Option<AfterReapHook>> = const { RefCell::new(None) };
}

/// Installs a one-shot test hook that runs after `wait` observes the exit.
#[cfg(test)]
fn hook_after_reap(hook: impl FnOnce(Option<&Cancel>) + 'static) {
    AFTER_REAP.with(|slot| *slot.borrow_mut() = Some(Box::new(hook)));
}

/// Runs and clears the test hook installed for the current thread.
#[cfg(test)]
fn after_reap(stop: Option<&Cancel>) {
    let hook = AFTER_REAP.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook(stop);
    }
}

#[derive(Clone, Copy)]
pub(super) struct Version {
    pub(super) major: u32,
    pub(super) minor: u32,
}

pub(super) struct CommandSpec<'a> {
    pub git: &'a Path,
    pub repo: Option<&'a Path>,
    /// An absolute git dir; the child runs from the temp folder instead of `-C <repo>`.
    pub git_dir: Option<&'a Path>,
    pub env: &'a [(OsString, OsString)],
    pub timeout: Duration,
}

pub(super) fn resolve_git(git: &Path, env: &[(OsString, OsString)], timeout: Duration) -> PathBuf {
    let spec = CommandSpec {
        git,
        repo: None,
        git_dir: None,
        env,
        timeout,
    };
    let Ok(output) = run_process(&spec, &["--exec-path"]) else {
        return git.to_path_buf();
    };
    if !output.status.success() {
        return git.to_path_buf();
    }
    let exec_path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if exec_path.is_empty() {
        return git.to_path_buf();
    }
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let candidate = Path::new(&exec_path).join(name);
    if fs::metadata(&candidate).is_ok_and(|metadata| metadata.is_file()) {
        candidate
    } else {
        git.to_path_buf()
    }
}

pub(super) fn version_of(
    git: &Path,
    env: &[(OsString, OsString)],
    timeout: Duration,
) -> Result<(Version, String), GitError> {
    let spec = CommandSpec {
        git,
        repo: None,
        git_dir: None,
        env,
        timeout,
    };
    let output = run_process(&spec, &["--version"])?;
    if !output.status.success() {
        return Err(GitError::Command {
            args: "--version".to_owned(),
            status: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        });
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let token = text
        .split_whitespace()
        .find(|token| token.chars().next().is_some_and(|c| c.is_ascii_digit()))
        .ok_or_else(protocol)?;
    let mut parts = token.split('.');
    let major = parse_number(parts.next()).ok_or_else(protocol)?;
    let minor = parse_number(parts.next()).ok_or_else(protocol)?;
    Ok((Version { major, minor }, text))
}

fn parse_number(part: Option<&str>) -> Option<u32> {
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

/// Builds the command for `spec`: detached to its git dir, or from the repository with `-C`.
pub(super) fn base_command(spec: &CommandSpec<'_>) -> Command {
    let mut command = Command::new(spec.git);
    if let Some(git_dir) = spec.git_dir {
        command
            .arg("--git-dir")
            .arg(git_dir)
            .current_dir(std::env::temp_dir());
    } else if let Some(repo) = spec.repo {
        command.arg("-C").arg(repo);
    }
    command
        .envs(spec.env.iter().map(|(key, value)| (key, value)))
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("LC_ALL", "C")
        .stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

pub(super) fn run_process<S: AsRef<OsStr>>(
    spec: &CommandSpec<'_>,
    args: &[S],
) -> Result<Output, GitError> {
    let ticket = spec
        .repo
        .map(|repo| running::register(repo, Work::Read))
        .transpose()?;
    let stop = ticket.as_ref().map(Ticket::stop_flag);
    let Spawned {
        mut tree,
        stdout,
        stderr,
    } = spawn(spec, args, ticket)?;
    let started = Instant::now();
    let status = match wait(&mut tree, spec, started, stop.as_ref()) {
        Ok(status) => status,
        Err(error) => {
            let _ = tree.stop();
            drop(stdout);
            drop(stderr);
            return Err(error);
        }
    };
    Ok(Output {
        status,
        stdout: stdout.finish(started, spec.timeout, spec.git)?,
        stderr: stderr.finish(started, spec.timeout, spec.git)?,
    })
}

struct Spawned {
    tree: GitTree,
    stdout: Reader,
    stderr: Reader,
}

struct Reader {
    receiver: mpsc::Receiver<io::Result<Vec<u8>>>,
    handle: JoinHandle<()>,
}

impl Reader {
    fn finish(self, started: Instant, timeout: Duration, git: &Path) -> Result<Vec<u8>, GitError> {
        let remaining = timeout.saturating_sub(started.elapsed());
        match self.receiver.recv_timeout(remaining) {
            Ok(Ok(bytes)) => {
                let _ = self.handle.join();
                Ok(bytes)
            }
            Ok(Err(error)) => {
                let _ = self.handle.join();
                Err(GitError::Io {
                    path: git.to_path_buf(),
                    kind: error.kind(),
                })
            }
            Err(RecvTimeoutError::Timeout) => Err(GitError::Timeout),
            Err(RecvTimeoutError::Disconnected) => Err(GitError::Io {
                path: git.to_path_buf(),
                kind: io::ErrorKind::Other,
            }),
        }
    }
}

/// Starts the child, attaches its registry ticket, and takes its pipes for the readers.
fn spawn<S: AsRef<OsStr>>(
    spec: &CommandSpec<'_>,
    args: &[S],
    ticket: Option<Ticket>,
) -> Result<Spawned, GitError> {
    let mut command = base_command(spec);
    command
        .args(args.iter())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut tree = GitTree::spawn(&mut command).map_err(|error| command_io(spec.git, error))?;
    tree.attach(ticket);
    let (stdout, stderr) = tree.take_pipes();
    let Some(stdout) = stdout else {
        let _ = tree.stop();
        return Err(protocol());
    };
    let Some(stderr) = stderr else {
        let _ = tree.stop();
        drop(stdout);
        return Err(protocol());
    };
    let stdout = match read_thread(stdout) {
        Ok(thread) => thread,
        Err(error) => {
            let _ = tree.stop();
            return Err(error);
        }
    };
    let stderr = match read_thread(stderr) {
        Ok(thread) => thread,
        Err(error) => {
            let _ = tree.stop();
            drop(stdout);
            return Err(error);
        }
    };
    Ok(Spawned {
        tree,
        stdout,
        stderr,
    })
}

/// Polls the tree, returning `Paused` when its stop flag is set and `Timeout` at the deadline.
fn wait(
    tree: &mut GitTree,
    spec: &CommandSpec<'_>,
    started: Instant,
    stop: Option<&Cancel>,
) -> Result<ExitStatus, GitError> {
    loop {
        if stop.is_some_and(Cancel::is_cancelled) {
            return Err(GitError::Paused);
        }
        match tree.try_wait() {
            Ok(Some(status)) => {
                #[cfg(test)]
                after_reap(stop);
                if stop.is_some_and(Cancel::is_cancelled) {
                    return Err(GitError::Paused);
                }
                return Ok(status);
            }
            Ok(None) => {}
            Err(error) => {
                return Err(GitError::Io {
                    path: spec.git.to_path_buf(),
                    kind: error.kind(),
                });
            }
        }
        if started.elapsed() >= spec.timeout {
            return Err(GitError::Timeout);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn read_thread<R>(mut reader: R) -> Result<Reader, GitError>
where
    R: Read + Send + 'static,
{
    let (sender, receiver) = mpsc::channel();
    let handle = thread::Builder::new()
        .name("git-reader".to_owned())
        .spawn(move || {
            let mut bytes = Vec::new();
            let result = reader.read_to_end(&mut bytes).map(|_| bytes);
            let _ = sender.send(result);
        })
        .map_err(|error| GitError::Io {
            path: PathBuf::from("git reader"),
            kind: error.kind(),
        })?;
    Ok(Reader { receiver, handle })
}

pub(super) fn command_io(git: &Path, error: io::Error) -> GitError {
    if error.kind() == io::ErrorKind::NotFound {
        GitError::GitNotFound
    } else {
        GitError::Io {
            path: git.to_path_buf(),
            kind: error.kind(),
        }
    }
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;

#[cfg(test)]
#[cfg(windows)]
#[path = "process_windows_tests.rs"]
mod windows_tests;
