use std::path::{Path, PathBuf};

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

fn always(_path: &Path) -> bool {
    true
}

fn never(_path: &Path) -> bool {
    false
}

fn windows(on_path: fn(&str) -> Option<PathBuf>, is_file: fn(&Path) -> bool) -> Probe {
    Probe {
        on_path,
        is_file,
        system_root: None,
        program_dirs: Vec::new(),
        wsl: vec![String::from("Ubuntu")],
        env_shell: Some(String::from("/usr/bin/fish")),
        os: Os::Windows,
    }
}

fn unix(os: Os, env_shell: Option<&str>) -> Probe {
    Probe {
        on_path: nothing,
        is_file: never,
        system_root: None,
        program_dirs: Vec::new(),
        wsl: Vec::new(),
        env_shell: env_shell.map(String::from),
        os,
    }
}

fn git_bash(probe: &Probe) -> Option<PathBuf> {
    detect(probe)
        .into_iter()
        .find(|shell| shell.kind == ShellKind::GitBash)
        .map(|shell| shell.program)
}

#[test]
fn windows_lists_pwsh_powershell_cmd_git_bash_then_wsl_distros() {
    let shells = detect(&windows(everything, always));
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
fn windows_skips_shells_that_are_not_on_path_or_installed() {
    assert!(detect(&windows(nothing, never)).is_empty());
}

fn git_in_root_bin(name: &str) -> Option<PathBuf> {
    (name == "git.exe").then(|| PathBuf::from("/Git/bin/git.exe"))
}

fn git_in_mingw(name: &str) -> Option<PathBuf> {
    (name == "git.exe").then(|| PathBuf::from("/Git/mingw64/bin/git.exe"))
}

fn only_usr_bash(path: &Path) -> bool {
    path.ends_with("usr/bin/bash.exe")
}

#[test]
fn git_bash_resolves_from_a_git_exe_in_the_install_roots_bin_folder() {
    let bash = git_bash(&windows(git_in_root_bin, always));
    assert_eq!(bash, Some(PathBuf::from("/Git/bin/bash.exe")));
}

#[test]
fn git_bash_resolves_from_mingw_and_falls_back_to_usr_bin() {
    let bash = git_bash(&windows(git_in_mingw, always));
    assert_eq!(bash, Some(PathBuf::from("/Git/bin/bash.exe")));
    let usr = git_bash(&windows(git_in_mingw, only_usr_bash));
    assert_eq!(
        usr,
        Some(Path::new("/Git").join("usr").join("bin").join("bash.exe"))
    );
}

#[test]
fn git_bash_is_not_offered_when_bash_is_missing() {
    assert_eq!(git_bash(&windows(git_in_root_bin, never)), None);
}

#[test]
fn windows_finds_shells_in_their_standard_folders_without_path() {
    let mut probe = windows(nothing, always);
    probe.system_root = Some(PathBuf::from("/Windows"));
    probe.program_dirs = vec![PathBuf::from("/Program Files")];
    let shells = detect(&probe);
    let programs: Vec<PathBuf> = shells.iter().map(|shell| shell.program.clone()).collect();
    let system32 = Path::new("/Windows").join("System32");
    let programs_dir = Path::new("/Program Files");
    assert_eq!(
        programs,
        [
            programs_dir.join("PowerShell").join("7").join("pwsh.exe"),
            system32
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe"),
            system32.join("cmd.exe"),
            programs_dir.join("Git").join("bin").join("bash.exe"),
            system32.join("wsl.exe"),
        ]
    );
}

#[test]
fn wsl_distro_names_stay_one_plain_argument() {
    let mut probe = windows(everything, always);
    probe.wsl = vec![String::from("My Distro; $(rm -rf ~)")];
    let wsl = detect(&probe)
        .into_iter()
        .find(|shell| shell.kind == ShellKind::Wsl);
    let wsl = wsl.map(|shell| shell.args);
    assert_eq!(
        wsl,
        Some(vec![
            String::from("-d"),
            String::from("My Distro; $(rm -rf ~)")
        ])
    );
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

fn only_usr_local(path: &Path) -> bool {
    path.starts_with("/usr/local")
}

#[test]
fn unix_fallback_finds_the_shell_in_a_standard_folder() {
    let mut probe = unix(Os::MacOs, None);
    probe.is_file = only_usr_local;
    assert_eq!(
        detect(&probe)[0].program,
        PathBuf::from("/usr/local/bin/zsh")
    );
}

#[test]
fn unix_prefers_the_shell_variable() {
    let shells = detect(&unix(Os::Linux, Some("/usr/bin/fish")));
    assert_eq!(shells.len(), 1);
    assert_eq!(shells[0].kind, ShellKind::Fish);
    assert_eq!(shells[0].label, "fish");
}
