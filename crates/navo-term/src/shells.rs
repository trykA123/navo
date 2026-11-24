//! Shell detection: which shells this machine can start, in D161.7's order.
//! `detect` is pure over a `Probe`; `Probe::system` is the only part that reads the machine.

use std::path::{Path, PathBuf};

/// The operating system a probe describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Windows,
    MacOs,
    Linux,
}

/// What kind of shell a `ShellSpec` starts; prompt integration differs per kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellKind {
    Pwsh,
    WindowsPowerShell,
    Cmd,
    GitBash,
    Wsl,
    Bash,
    Zsh,
    Fish,
    Other,
}

/// A shell the user can start: the program, its arguments and the name shown in the picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShellSpec {
    pub kind: ShellKind,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub label: String,
}

/// Everything `detect` needs to know about the machine.
#[derive(Clone, Debug)]
pub struct Probe {
    /// Finds a program by file name on `PATH`.
    pub on_path: fn(&str) -> Option<PathBuf>,
    /// Whether a file exists; checks standard install locations and Git Bash.
    pub is_file: fn(&Path) -> bool,
    /// Windows: the `SystemRoot` folder (System32's parent).
    pub system_root: Option<PathBuf>,
    /// Windows: the Program Files folders and the per-user programs folder.
    pub program_dirs: Vec<PathBuf>,
    /// Installed WSL distro names, in the order WSL lists them.
    pub wsl: Vec<String>,
    /// The `SHELL` environment variable (Unix only).
    pub env_shell: Option<String>,
    pub os: Os,
}

impl Probe {
    /// Describes this machine. `wsl` comes from the caller (the drive list already reads it).
    /// `on_path` and `is_file` touch the disk, so call `detect` off the UI thread.
    pub fn system(wsl: Vec<String>) -> Self {
        let os = if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        };
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
        let mut program_dirs: Vec<PathBuf> = ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"]
            .into_iter()
            .filter_map(var)
            .chain(var("LOCALAPPDATA").map(|dir| dir.join("Programs")))
            .collect();
        program_dirs.dedup();
        Self {
            on_path: search_path,
            is_file: Path::is_file,
            system_root: var("SystemRoot"),
            program_dirs,
            wsl,
            env_shell: std::env::var("SHELL").ok(),
            os,
        }
    }

    /// `name` on `PATH`, else the first standard location that exists.
    fn find(&self, name: &str, standard: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
        (self.on_path)(name).or_else(|| standard.into_iter().find(|path| (self.is_file)(path)))
    }

    /// `<SystemRoot>\System32\<rest>`.
    fn system32(&self, rest: &[&str]) -> Option<PathBuf> {
        let mut path = self.system_root.as_ref()?.join("System32");
        path.extend(rest);
        Some(path)
    }
}

/// Lists the shells that can start, best first: Windows pwsh, Windows PowerShell, cmd,
/// Git Bash, then each WSL distro; Unix `$SHELL`, else zsh (macOS) or bash (Linux).
pub fn detect(probe: &Probe) -> Vec<ShellSpec> {
    match probe.os {
        Os::Windows => detect_windows(probe),
        Os::MacOs => vec![unix_shell(probe, "zsh")],
        Os::Linux => vec![unix_shell(probe, "bash")],
    }
}

fn detect_windows(probe: &Probe) -> Vec<ShellSpec> {
    let logo = || vec![String::from("-NoLogo")];
    let pwsh = probe
        .program_dirs
        .iter()
        .map(|dir| dir.join("PowerShell").join("7").join("pwsh.exe"));
    let powershell = probe.system32(&["WindowsPowerShell", "v1.0", "powershell.exe"]);
    let mut shells: Vec<ShellSpec> = [
        probe
            .find("pwsh.exe", pwsh)
            .map(|program| spec(ShellKind::Pwsh, program, logo(), "PowerShell")),
        probe.find("powershell.exe", powershell).map(|program| {
            spec(
                ShellKind::WindowsPowerShell,
                program,
                logo(),
                "Windows PowerShell",
            )
        }),
        probe
            .find("cmd.exe", probe.system32(&["cmd.exe"]))
            .map(|program| spec(ShellKind::Cmd, program, Vec::new(), "Command Prompt")),
        git_bash(probe).map(|program| {
            let args = vec![String::from("--login"), String::from("-i")];
            spec(ShellKind::GitBash, program, args, "Git Bash")
        }),
    ]
    .into_iter()
    .flatten()
    .collect();
    if let Some(wsl) = probe.find("wsl.exe", probe.system32(&["wsl.exe"])) {
        // Distro names stay plain arguments: nothing interprets them as shell text.
        for distro in &probe.wsl {
            let args = vec![String::from("-d"), distro.clone()];
            shells.push(spec(ShellKind::Wsl, wsl.clone(), args, distro));
        }
    }
    shells
}

/// Git Bash from the Git install root that holds `git.exe` on `PATH`, else from Git's
/// standard install folders: `<root>\bin\bash.exe` or `<root>\usr\bin\bash.exe`, whichever
/// exists.
fn git_bash(probe: &Probe) -> Option<PathBuf> {
    let from_path = (probe.on_path)("git.exe").and_then(|git| git_root(&git));
    let standard = probe.program_dirs.iter().map(|dir| dir.join("Git"));
    from_path.into_iter().chain(standard).find_map(|root| {
        [root.join("bin"), root.join("usr").join("bin")]
            .into_iter()
            .map(|dir| dir.join("bash.exe"))
            .find(|bash| (probe.is_file)(bash))
    })
}

/// Git for Windows puts `git.exe` in `<root>\cmd`, `<root>\bin` or `<root>\mingw64\bin`
/// (also `mingw32`, `clangarm64`).
fn git_root(git: &Path) -> Option<PathBuf> {
    let dir = git.parent()?;
    let root = dir.parent()?;
    let named = |path: &Path, names: &[&str]| {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| names.iter().any(|want| name.eq_ignore_ascii_case(want)))
    };
    if named(dir, &["cmd"]) {
        Some(root.to_path_buf())
    } else if named(dir, &["bin"]) && named(root, &["mingw64", "mingw32", "clangarm64"]) {
        root.parent().map(Path::to_path_buf)
    } else if named(dir, &["bin"]) {
        Some(root.to_path_buf())
    } else {
        None
    }
}

fn unix_shell(probe: &Probe, fallback: &str) -> ShellSpec {
    let standard = ["/bin", "/usr/bin", "/usr/local/bin", "/opt/homebrew/bin"]
        .map(|dir| Path::new(dir).join(fallback));
    let program = match probe.env_shell.as_deref().filter(|shell| !shell.is_empty()) {
        Some(shell) => PathBuf::from(shell),
        None => probe
            .find(fallback, standard)
            .unwrap_or_else(|| Path::new("/bin").join(fallback)),
    };
    let label = program.file_name().map_or_else(
        || String::from(fallback),
        |name| name.to_string_lossy().into_owned(),
    );
    let kind = match label.as_str() {
        "bash" => ShellKind::Bash,
        "zsh" => ShellKind::Zsh,
        "fish" => ShellKind::Fish,
        _ => ShellKind::Other,
    };
    spec(kind, program, Vec::new(), &label)
}

fn spec(kind: ShellKind, program: PathBuf, args: Vec<String>, label: &str) -> ShellSpec {
    ShellSpec {
        kind,
        program,
        args,
        label: String::from(label),
    }
}

/// The real `on_path`: the first `PATH` entry that holds a file with this name.
fn search_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}
