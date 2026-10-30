use std::ffi::{OsStr, OsString};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::GitError;

use super::parse::protocol;

#[derive(Clone, Copy)]
pub(super) struct Version {
    pub(super) major: u32,
    pub(super) minor: u32,
}

pub(super) struct CommandSpec<'a> {
    pub git: &'a Path,
    pub repo: Option<&'a Path>,
    pub env: &'a [(OsString, OsString)],
    pub timeout: Duration,
}

pub(super) fn find_git() -> Result<PathBuf, GitError> {
    const NAME: &str = if cfg!(windows) { "git.exe" } else { "git" };
    let Some(paths) = std::env::var_os("PATH") else {
        return Err(GitError::GitNotFound);
    };
    for folder in std::env::split_paths(&paths) {
        let candidate = folder.join(NAME);
        match fs::metadata(&candidate) {
            Ok(metadata) if metadata.is_file() => return Ok(candidate),
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(GitError::Io {
                    path: candidate,
                    kind: error.kind(),
                });
            }
        }
    }
    Err(GitError::GitNotFound)
}

pub(super) fn resolve_git(git: &Path, env: &[(OsString, OsString)], timeout: Duration) -> PathBuf {
    let spec = CommandSpec {
        git,
        repo: None,
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

pub(super) fn base_command(spec: &CommandSpec<'_>) -> Command {
    let mut command = Command::new(spec.git);
    if let Some(repo) = spec.repo {
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
    let Spawned {
        mut child,
        stdout,
        stderr,
    } = spawn(spec, args)?;
    let started = Instant::now();
    let status = match wait(&mut child, spec.git, started, spec.timeout) {
        Ok(status) => status,
        Err(error) => {
            stop(&mut child);
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
    child: Child,
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

fn spawn<S: AsRef<OsStr>>(spec: &CommandSpec<'_>, args: &[S]) -> Result<Spawned, GitError> {
    let mut command = base_command(spec);
    command
        .args(args.iter())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| command_io(spec.git, error))?;
    let Some(stdout) = child.stdout.take() else {
        stop(&mut child);
        return Err(protocol());
    };
    let Some(stderr) = child.stderr.take() else {
        stop(&mut child);
        return Err(protocol());
    };
    let stdout = match read_thread(stdout) {
        Ok(thread) => thread,
        Err(error) => {
            stop(&mut child);
            return Err(error);
        }
    };
    let stderr = match read_thread(stderr) {
        Ok(thread) => thread,
        Err(error) => {
            stop(&mut child);
            drop(stdout);
            return Err(error);
        }
    };
    Ok(Spawned {
        child,
        stdout,
        stderr,
    })
}

fn wait(
    child: &mut Child,
    git: &Path,
    started: Instant,
    timeout: Duration,
) -> Result<ExitStatus, GitError> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(error) => {
                return Err(GitError::Io {
                    path: git.to_path_buf(),
                    kind: error.kind(),
                });
            }
        }
        if started.elapsed() >= timeout {
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

fn stop(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
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
