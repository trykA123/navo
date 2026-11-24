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
    /// Installed WSL distro names, in the order WSL lists them.
    pub wsl: Vec<String>,
    /// The `SHELL` environment variable (Unix only).
    pub env_shell: Option<String>,
    pub os: Os,
}

impl Probe {
    /// Describes this machine. `wsl` comes from the caller (the drive list already reads it).
    /// `on_path` touches the disk, so call `detect` off the UI thread.
    pub fn system(wsl: Vec<String>) -> Self {
        let os = if cfg!(windows) {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::MacOs
        } else {
            Os::Linux
        };
        Self {
            on_path: search_path,
            wsl,
            env_shell: std::env::var("SHELL").ok(),
            os,
        }
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
    let fixed = [
        (ShellKind::Pwsh, "pwsh.exe", "PowerShell"),
        (
            ShellKind::WindowsPowerShell,
            "powershell.exe",
            "Windows PowerShell",
        ),
        (ShellKind::Cmd, "cmd.exe", "Command Prompt"),
    ];
    let mut shells: Vec<ShellSpec> = fixed
        .into_iter()
        .filter_map(|(kind, name, label)| {
            let program = (probe.on_path)(name)?;
            let args = match kind {
                ShellKind::Cmd => Vec::new(),
                _ => vec![String::from("-NoLogo")],
            };
            Some(spec(kind, program, args, label))
        })
        .collect();
    if let Some(bash) = (probe.on_path)("git.exe").and_then(|git| git_bash(&git)) {
        let args = vec![String::from("--login"), String::from("-i")];
        shells.push(spec(ShellKind::GitBash, bash, args, "Git Bash"));
    }
    if let Some(wsl) = (probe.on_path)("wsl.exe") {
        for distro in &probe.wsl {
            let args = vec![String::from("-d"), distro.clone()];
            shells.push(spec(ShellKind::Wsl, wsl.clone(), args, distro));
        }
    }
    shells
}

/// Git for Windows puts `git.exe` in `<root>\cmd` or `<root>\mingw64\bin`; Git Bash is
/// `<root>\bin\bash.exe`.
fn git_bash(git: &Path) -> Option<PathBuf> {
    let dir = git.parent()?;
    let name = dir.file_name()?.to_str()?.to_ascii_lowercase();
    let root = match name.as_str() {
        "cmd" => dir.parent()?,
        "bin" => dir.parent()?.parent()?,
        _ => return None,
    };
    Some(root.join("bin").join("bash.exe"))
}

fn unix_shell(probe: &Probe, fallback: &str) -> ShellSpec {
    let program = match probe.env_shell.as_deref().filter(|shell| !shell.is_empty()) {
        Some(shell) => PathBuf::from(shell),
        None => (probe.on_path)(fallback).unwrap_or_else(|| Path::new("/bin").join(fallback)),
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
