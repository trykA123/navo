use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Once;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

static NEXT: AtomicU64 = AtomicU64::new(0);
static ISOLATE: Once = Once::new();

const GIT_TIMEOUT: Duration = Duration::from_secs(60);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(name: &str) -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("navo-git-{name}-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.path).unwrap_or_default();
    }
}

pub struct Fixture {
    _root: TempDir,
    git: PathBuf,
    repo: PathBuf,
}

#[allow(
    dead_code,
    reason = "each integration test binary uses only some fixture helpers"
)]
impl Fixture {
    pub fn new() -> Result<Self, String> {
        ISOLATE.call_once(close_inherited_on_exec);
        let git = resolve_git(find_git().ok_or_else(|| "git was not found on PATH".to_owned())?);
        let version = version_of(&git)?;
        if version < (2, 36) {
            return Err(format!("git {version:?} is older than 2.36"));
        }
        let root = TempDir::new("cli");
        let repo = root.path.join("repo");
        fs::create_dir_all(&repo).unwrap();
        Ok(Self {
            _root: root,
            git,
            repo,
        })
    }

    pub fn repo(&self) -> &Path {
        &self.repo
    }

    pub fn version(&self) -> (u32, u32) {
        version_of(&self.git).unwrap()
    }

    pub fn try_init(&self, args: &[&str]) -> Output {
        let mut all = vec!["init"];
        all.extend_from_slice(args);
        self.git(&all)
    }

    pub fn init(&self, args: &[&str]) {
        let output = self.try_init(args);
        assert!(
            output.status.success(),
            "git init failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    pub fn write(&self, relative: &str, bytes: &[u8]) {
        let path = self.repo.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    pub fn git(&self, args: &[&str]) -> Output {
        run(self.command().args(args), None)
    }

    pub fn git_ok(&self, args: &[&str]) -> Output {
        let output = self.git(args);
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    pub fn git_stdin(&self, args: &[&str], input: &[u8]) -> Output {
        run(self.command().args(args), Some(input.to_vec()))
    }

    pub fn git_stdin_ok(&self, args: &[&str], input: &[u8]) -> Output {
        let output = self.git_stdin(args, input);
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    pub fn add_blob(&self, path: &str, bytes: &[u8]) -> String {
        let output = self.git_stdin_ok(&["hash-object", "-w", "--stdin"], bytes);
        let oid = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        self.git_ok(&[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{oid},{path}"),
        ]);
        oid
    }

    pub fn commit(&self, message: &str) {
        self.git_ok(&["commit", "-m", message]);
    }

    #[cfg(unix)]
    pub fn script(&self, name: &str, content: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;

        let path = self._root.path.join(name);
        fs::write(&path, content).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path
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

/// Kills and reaps the child when a test panics or times out, so no git outlives the test.
struct Reaped(Child);

impl Drop for Reaped {
    fn drop(&mut self) {
        self.0.kill().unwrap_or_default();
        self.0.wait().map(drop).unwrap_or_default();
    }
}

fn run(command: &mut Command, input: Option<Vec<u8>>) -> Output {
    let stdin = if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    };
    command
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = Reaped(command.spawn().unwrap());
    let stdout = drain(child.0.stdout.take().unwrap());
    let stderr = drain(child.0.stderr.take().unwrap());
    if let (Some(input), Some(mut pipe)) = (input, child.0.stdin.take()) {
        thread::spawn(move || pipe.write_all(&input).unwrap_or_default());
    }
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        assert!(
            started.elapsed() < GIT_TIMEOUT,
            "git timed out: {command:?}"
        );
        thread::sleep(Duration::from_millis(2));
    };
    Output {
        status,
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    }
}

fn drain(mut pipe: impl Read + Send + 'static) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        pipe.read_to_end(&mut bytes).unwrap_or_default();
        bytes
    })
}

/// Marks every inherited descriptor above stderr close-on-exec (a cargo lock among them),
/// so no git child holds it.
#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "the listed descriptors are marked close-on-exec"
)]
fn close_inherited_on_exec() {
    use rustix::fd::BorrowedFd;
    use rustix::io::{FdFlags, fcntl_getfd, fcntl_setfd};

    let folder = if cfg!(target_os = "linux") {
        "/proc/self/fd"
    } else {
        "/dev/fd"
    };
    let Ok(entries) = fs::read_dir(folder) else {
        return;
    };
    let fds: Vec<i32> = entries
        .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
        .filter(|fd| *fd > 2)
        .collect();
    for fd in fds {
        // SAFETY: the descriptor was listed as open and nothing in the tests closes inherited ones.
        let fd = unsafe { BorrowedFd::borrow_raw(fd) };
        if let Ok(flags) = fcntl_getfd(fd) {
            fcntl_setfd(fd, flags | FdFlags::CLOEXEC).unwrap_or_default();
        }
    }
}

#[cfg(not(unix))]
fn close_inherited_on_exec() {}

fn resolve_git(git: PathBuf) -> PathBuf {
    let output = run(Command::new(&git).arg("--exec-path"), None);
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
    for folder in std::env::split_paths(&paths) {
        let candidate = folder.join(name);
        if fs::metadata(&candidate)
            .map(|meta| meta.is_file())
            .unwrap_or(false)
        {
            return Some(candidate);
        }
    }
    None
}

fn version_of(git: &Path) -> Result<(u32, u32), String> {
    let output = run(Command::new(git).arg("--version"), None);
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
