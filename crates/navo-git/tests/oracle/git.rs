use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) fn env_repositories() -> Vec<PathBuf> {
    let Some(value) = std::env::var_os("NAVO_ORACLE_REPOS") else {
        return Vec::new();
    };
    std::env::split_paths(&value)
        .filter(|path| !path.as_os_str().is_empty())
        .collect()
}

pub(crate) fn worktree_root(git: &Path) -> PathBuf {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let output = git_output(git, manifest, &["rev-parse", "--show-toplevel"]);
    let root = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if root.is_empty() {
        panic!(
            "oracle: git rev-parse --show-toplevel in {} printed nothing",
            manifest.display()
        );
    }
    PathBuf::from(root)
}

pub(crate) fn find_git() -> Option<PathBuf> {
    let name = if cfg!(windows) { "git.exe" } else { "git" };
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|folder| folder.join(name))
        .find(|candidate| candidate.is_file())
}

pub(crate) fn git_version(git: &Path) -> Result<(u32, u32), String> {
    let output = run_git(git, None, &["--version"])?;
    if !output.status.success() {
        return Err(format!(
            "git --version failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let token = text
        .split_whitespace()
        .find(|token| token.starts_with(|c: char| c.is_ascii_digit()))
        .ok_or("git did not print a version")?;
    let mut parts = token.split('.');
    let major = version_number(parts.next()).ok_or("git did not print a version")?;
    let minor = version_number(parts.next()).ok_or("git did not print a version")?;
    Ok((major, minor))
}

pub(crate) fn commit_list(git: &Path, repo: &Path, head: &str) -> (Vec<String>, Vec<String>) {
    let commits = rev_list(git, repo, &["rev-list", "--max-count=100", head]);
    let merges = rev_list(git, repo, &["rev-list", "--merges", head]);
    let shallow = match git_text(git, repo, &["rev-parse", "--is-shallow-repository"]).trim() {
        "true" => true,
        "false" => false,
        other => panic!(
            "oracle: git rev-parse --is-shallow-repository in {} printed {other:?}",
            repo.display()
        ),
    };
    let total = git_text(git, repo, &["rev-list", "--count", head])
        .trim()
        .parse::<usize>()
        .unwrap_or_else(|error| {
            panic!(
                "oracle: git rev-list --count in {}: {error}",
                repo.display()
            )
        });
    if shallow {
        say(&format!(
            "{}: shallow clone, {} commits, {} merges",
            repo.display(),
            commits.len(),
            merges.len()
        ));
    } else if total < 100 {
        say(&format!(
            "{}: {} commits, fewer than 100; {} merges",
            repo.display(),
            total,
            merges.len()
        ));
    } else {
        say(&format!(
            "{}: last 100 commits, {} merges",
            repo.display(),
            merges.len()
        ));
    }
    (commits, merges)
}

pub(crate) fn merge_parent_pairs(git: &Path, repo: &Path, commit: &str) -> Vec<(String, String)> {
    let output = git_output(git, repo, &["rev-list", "--parents", "-n", "1", commit]);
    let text = String::from_utf8_lossy(&output.stdout);
    let mut parents = text.split_whitespace().skip(1);
    let Some(first) = parents.next() else {
        return Vec::new();
    };
    parents
        .map(|parent| (first.to_owned(), parent.to_owned()))
        .collect()
}

pub(crate) fn git_text(git: &Path, repo: &Path, args: &[&str]) -> String {
    String::from_utf8_lossy(&git_output(git, repo, args).stdout).into_owned()
}

pub(crate) fn git_output(git: &Path, repo: &Path, args: &[&str]) -> Output {
    let output = run_git(git, Some(repo), args).unwrap_or_else(|error| panic!("oracle: {error}"));
    if !output.status.success() {
        panic!(
            "oracle: git {args:?} in {} failed with {}: {}",
            repo.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    output
}

pub(crate) fn merge_base_all(git: &Path, repo: &Path, left: &str, right: &str) -> HashSet<String> {
    let output = run_git(git, Some(repo), &["merge-base", "--all", left, right])
        .unwrap_or_else(|error| panic!("oracle: {error}"));
    if !output.status.success() && output.status.code() != Some(1) {
        panic!(
            "oracle: git merge-base --all {left} {right} in {} failed with {}: {}",
            repo.display(),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(crate) fn say(message: &str) {
    let mut stderr = std::io::stderr();
    writeln!(stderr, "oracle: {message}").unwrap_or_default();
}

fn version_number(part: Option<&str>) -> Option<u32> {
    let digits: String = part?
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn rev_list(git: &Path, repo: &Path, args: &[&str]) -> Vec<String> {
    git_text(git, repo, args)
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn run_git(git: &Path, repo: Option<&Path>, args: &[&str]) -> Result<Output, String> {
    let mut command = Command::new(git);
    if let Some(repo) = repo {
        command.arg("-C").arg(repo);
    }
    command
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("git {args:?} in {repo:?}: {error}"))?;
    let Some(stdout) = child.stdout.take() else {
        stop_child(&mut child);
        return Err(format!("git {args:?} in {repo:?}: no stdout pipe"));
    };
    let Some(stderr) = child.stderr.take() else {
        stop_child(&mut child);
        return Err(format!("git {args:?} in {repo:?}: no stderr pipe"));
    };
    let started = Instant::now();
    let stdout_reader = spawn_reader(stdout);
    let stderr_reader = spawn_reader(stderr);
    let status = wait_child(&mut child, args, repo)?;
    let stdout = collect(&stdout_reader, started, args, repo)?;
    let stderr = collect(&stderr_reader, started, args, repo)?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn wait_child(child: &mut Child, args: &[&str], repo: Option<&Path>) -> Result<ExitStatus, String> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(error) => {
                stop_child(child);
                return Err(format!("git {args:?} in {repo:?}: {error}"));
            }
        }
        if started.elapsed() >= COMMAND_TIMEOUT {
            stop_child(child);
            return Err(format!(
                "git {args:?} in {repo:?} timed out after {}s",
                COMMAND_TIMEOUT.as_secs()
            ));
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn stop_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn read_pipe(mut pipe: impl Read) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    pipe.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn spawn_reader(pipe: impl Read + Send + 'static) -> Receiver<std::io::Result<Vec<u8>>> {
    let (send, receive) = mpsc::channel();
    thread::spawn(move || send.send(read_pipe(pipe)).unwrap_or_default());
    receive
}

fn collect(
    reader: &Receiver<std::io::Result<Vec<u8>>>,
    started: Instant,
    args: &[&str],
    repo: Option<&Path>,
) -> Result<Vec<u8>, String> {
    let left = COMMAND_TIMEOUT.saturating_sub(started.elapsed());
    match reader.recv_timeout(left) {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(error)) => Err(format!("git {args:?} in {repo:?}: {error}")),
        Err(RecvTimeoutError::Timeout) => Err(format!(
            "git {args:?} in {repo:?}: output still open after {}s",
            COMMAND_TIMEOUT.as_secs()
        )),
        Err(RecvTimeoutError::Disconnected) => {
            Err(format!("git {args:?} in {repo:?}: output reader panicked"))
        }
    }
}
