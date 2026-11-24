use std::path::PathBuf;

use crate::shells::{Os, Probe, ShellKind, detect};

fn everything(name: &str) -> Option<PathBuf> {
    let dir = match name {
        "git.exe" => "/Program Files/Git/cmd",
        _ => "/Windows/System32",
    };
    Some(PathBuf::from(dir).join(name))
}

fn nothing(_name: &str) -> Option<PathBuf> {
    None
}

fn windows(on_path: fn(&str) -> Option<PathBuf>) -> Probe {
    Probe {
        on_path,
        wsl: vec![String::from("Ubuntu")],
        env_shell: Some(String::from("/usr/bin/fish")),
        os: Os::Windows,
    }
}

fn unix(os: Os, env_shell: Option<&str>) -> Probe {
    Probe {
        on_path: nothing,
        wsl: Vec::new(),
        env_shell: env_shell.map(String::from),
        os,
    }
}

#[test]
fn windows_lists_pwsh_powershell_cmd_git_bash_then_wsl_distros() {
    let shells = detect(&windows(everything));
    let kinds: Vec<ShellKind> = shells.iter().map(|shell| shell.kind).collect();
    assert_eq!(
        kinds,
        [
            ShellKind::Pwsh,
            ShellKind::WindowsPowerShell,
            ShellKind::Cmd,
            ShellKind::GitBash,
            ShellKind::Wsl,
        ]
    );
    assert_eq!(
        shells[3].program,
        PathBuf::from("/Program Files/Git/bin/bash.exe")
    );
    assert_eq!(shells[4].label, "Ubuntu");
    assert_eq!(shells[4].args, ["-d", "Ubuntu"]);
}

#[test]
fn windows_skips_shells_that_are_not_on_path() {
    assert!(detect(&windows(nothing)).is_empty());
}

#[test]
fn unix_without_shell_variable_falls_back_per_os() {
    let mac = detect(&unix(Os::MacOs, None));
    let linux = detect(&unix(Os::Linux, Some("")));
    assert_eq!(mac[0].kind, ShellKind::Zsh);
    assert_eq!(mac[0].program, PathBuf::from("/bin/zsh"));
    assert_eq!(linux[0].kind, ShellKind::Bash);
    assert_eq!(linux[0].program, PathBuf::from("/bin/bash"));
}

#[test]
fn unix_prefers_the_shell_variable() {
    let shells = detect(&unix(Os::Linux, Some("/usr/bin/fish")));
    assert_eq!(shells.len(), 1);
    assert_eq!(shells[0].kind, ShellKind::Fish);
    assert_eq!(shells[0].label, "fish");
}
