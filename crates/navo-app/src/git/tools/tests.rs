use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use navo_shell::Hive;
use navo_store::GitSettings;

use super::env::{SystemEnv, ToolEnv};
use super::{Capability, DetectedTool, ToolKind, choose, detect};

const PF: &str = "PF";
const PF86: &str = "PF86";
const LOCAL: &str = "LOCAL";

#[derive(Default)]
struct Fake {
    linux: bool,
    registry: HashMap<(bool, String, String), String>,
    files: HashSet<PathBuf>,
    path: HashMap<String, PathBuf>,
}

impl Fake {
    fn windows() -> Self {
        Self::default()
    }

    fn file(mut self, root: &str, rest: &str) -> Self {
        self.files.insert(Path::new(root).join(rest));
        self
    }

    fn reg(mut self, at: (Hive, &str, &str), text: &str) -> Self {
        let (hive, key, value) = at;
        let local_machine = hive == Hive::LocalMachine;
        self.registry.insert(
            (local_machine, key.to_owned(), value.to_owned()),
            text.to_owned(),
        );
        self
    }

    fn exists(mut self, at: &str) -> Self {
        self.files.insert(PathBuf::from(at));
        self
    }

    fn on_path(mut self, program: &str, at: &str) -> Self {
        self.files.insert(PathBuf::from(at));
        self.path.insert(program.to_owned(), PathBuf::from(at));
        self
    }
}

impl ToolEnv for Fake {
    fn windows(&self) -> bool {
        !self.linux
    }

    fn var(&self, name: &str) -> Option<String> {
        match name {
            "ProgramFiles" => Some(PF.to_owned()),
            "ProgramFiles(x86)" => Some(PF86.to_owned()),
            "LOCALAPPDATA" => Some(LOCAL.to_owned()),
            _ => None,
        }
    }

    fn registry(&self, hive: Hive, key: &str, value: &str) -> Option<String> {
        let local_machine = hive == Hive::LocalMachine;
        self.registry
            .get(&(local_machine, key.to_owned(), value.to_owned()))
            .cloned()
    }

    fn is_file(&self, path: &Path) -> bool {
        self.files.contains(path)
    }

    fn on_path(&self, program: &str) -> Option<PathBuf> {
        self.path.get(program).cloned()
    }
}

fn kinds(found: &[DetectedTool]) -> Vec<ToolKind> {
    found.iter().map(|tool| tool.kind).collect()
}

fn exe_of(found: &[DetectedTool], kind: ToolKind) -> Option<PathBuf> {
    found
        .iter()
        .find(|tool| tool.kind == kind)
        .map(|tool| tool.exe.clone())
}

fn settings(tool: &str, command: Option<&str>) -> GitSettings {
    GitSettings {
        tool: tool.to_owned(),
        tool_command: command.map(str::to_owned),
        ..GitSettings::default()
    }
}

const TORTOISE_KEY: &str = r"SOFTWARE\TortoiseGit";
const TORTOISE_REL: &str = r"TortoiseGit\bin\TortoiseGitProc.exe";

#[test]
fn tortoisegit_is_found_through_its_registry_value() {
    let exe = r"D:\Tools\TortoiseGit\bin\TortoiseGitProc.exe";
    let env = Fake::windows()
        .reg((Hive::LocalMachine, TORTOISE_KEY, "ProcPath"), exe)
        .exists(exe);
    let found = detect(&env);
    assert_eq!(
        exe_of(&found, ToolKind::TortoiseGit),
        Some(PathBuf::from(exe))
    );
}

#[test]
fn tortoisegit_falls_back_to_program_files_when_the_key_is_missing() {
    let env = Fake::windows().file(PF, TORTOISE_REL);
    let found = detect(&env);
    assert_eq!(
        exe_of(&found, ToolKind::TortoiseGit),
        Some(Path::new(PF).join(TORTOISE_REL))
    );
}

#[test]
fn a_stale_registry_value_does_not_hide_an_installed_tool() {
    let env = Fake::windows()
        .reg(
            (Hive::LocalMachine, TORTOISE_KEY, "ProcPath"),
            r"E:\gone\TortoiseGitProc.exe",
        )
        .file(PF86, TORTOISE_REL);
    let found = detect(&env);
    assert_eq!(
        exe_of(&found, ToolKind::TortoiseGit),
        Some(Path::new(PF86).join(TORTOISE_REL))
    );
}

#[test]
fn the_install_folder_value_joins_the_known_exe_names() {
    let exe = Path::new(r"D:\Tools\TortoiseGit").join(r"bin\TortoiseGitProc.exe");
    let mut env = Fake::windows().reg(
        (Hive::LocalMachine, TORTOISE_KEY, "Directory"),
        r"D:\Tools\TortoiseGit",
    );
    env.files.insert(exe.clone());
    assert_eq!(exe_of(&detect(&env), ToolKind::TortoiseGit), Some(exe));
}

#[test]
fn a_tool_is_found_in_its_per_user_folder_and_on_the_path() {
    let by_folder = Fake::windows().file(LOCAL, r"Fork\Fork.exe");
    assert_eq!(
        exe_of(&detect(&by_folder), ToolKind::Fork),
        Some(Path::new(LOCAL).join(r"Fork\Fork.exe"))
    );
    let by_path = Fake::windows().on_path("fork", r"X:\bin\fork.exe");
    assert_eq!(
        exe_of(&detect(&by_path), ToolKind::Fork),
        Some(PathBuf::from(r"X:\bin\fork.exe"))
    );
}

#[test]
fn an_uninstall_key_gives_the_exe_from_an_icon_or_an_install_folder() {
    let uninstall = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";
    let exe = r"D:\Apps\SourceTree\SourceTree.exe";
    let icon = Fake::windows()
        .reg(
            (
                Hive::CurrentUser,
                &format!(r"{uninstall}\SourceTree"),
                "DisplayIcon",
            ),
            &format!("{exe},0"),
        )
        .exists(exe);
    assert_eq!(
        exe_of(&detect(&icon), ToolKind::SourceTree),
        Some(PathBuf::from(exe))
    );
    let mut folder = Fake::windows().reg(
        (
            Hive::LocalMachine,
            &format!(r"{uninstall}\GitHubDesktop"),
            "InstallLocation",
        ),
        r"D:\Apps\GHD",
    );
    let want = Path::new(r"D:\Apps\GHD").join("GitHubDesktop.exe");
    folder.files.insert(want.clone());
    assert_eq!(
        exe_of(&detect(&folder), ToolKind::GitHubDesktop),
        Some(want)
    );
}

#[test]
fn a_url_handler_command_gives_the_exe() {
    let exe = r"D:\Apps\GitHub Desktop\GitHubDesktop.exe";
    let env = Fake::windows()
        .reg(
            (
                Hive::CurrentUser,
                r"Software\Classes\x-github-client\shell\open\command",
                "",
            ),
            &format!("\"{exe}\" --protocol-launcher \"%1\""),
        )
        .exists(exe);
    assert_eq!(
        exe_of(&detect(&env), ToolKind::GitHubDesktop),
        Some(PathBuf::from(exe))
    );
}

#[test]
fn the_other_open_only_tools_are_found_by_their_usual_places() {
    let env = Fake::windows()
        .file(LOCAL, r"gitkraken\gitkraken.exe")
        .file(PF, r"Sublime Merge\smerge.exe")
        .file(LOCAL, r"Programs\Microsoft VS Code\Code.exe");
    let found = detect(&env);
    for kind in [
        ToolKind::GitKraken,
        ToolKind::SublimeMerge,
        ToolKind::VsCode,
    ] {
        assert!(exe_of(&found, kind).is_some(), "{kind:?}");
    }
}

#[test]
fn nothing_installed_and_no_registry_finds_nothing() {
    assert!(detect(&Fake::windows()).is_empty());
}

#[test]
fn git_is_found_on_the_path() {
    let env = Fake::windows().on_path("git", r"C:\Git\cmd\git.exe");
    assert_eq!(kinds(&detect(&env)), [ToolKind::GitCli]);
}

#[test]
fn linux_detects_only_the_command_line() {
    let mut env = Fake::windows()
        .on_path("git", "/usr/bin/git")
        .on_path("code", "/usr/bin/code")
        .file(LOCAL, r"Fork\Fork.exe");
    env.linux = true;
    assert_eq!(kinds(&detect(&env)), [ToolKind::GitCli]);
}

fn all_installed() -> Fake {
    Fake::windows()
        .file(LOCAL, r"gitkraken\gitkraken.exe")
        .file(LOCAL, r"Programs\Microsoft VS Code\Code.exe")
        .file(LOCAL, r"Fork\Fork.exe")
        .file(PF, TORTOISE_REL)
        .on_path("git", r"C:\Git\cmd\git.exe")
}

#[test]
fn detection_lists_tools_in_the_auto_order_with_git_last() {
    let found = detect(&all_installed());
    assert_eq!(
        kinds(&found),
        [
            ToolKind::TortoiseGit,
            ToolKind::Fork,
            ToolKind::GitKraken,
            ToolKind::VsCode,
            ToolKind::GitCli
        ]
    );
}

#[test]
fn auto_picks_the_first_gui_tool_then_git_and_never_an_editor() {
    let found = detect(&all_installed());
    let pick = choose(&settings("auto", None), &found).unwrap();
    assert_eq!(pick.kind, ToolKind::TortoiseGit);
    let no_tortoise: Vec<_> = found
        .into_iter()
        .filter(|tool| tool.kind != ToolKind::TortoiseGit)
        .collect();
    let pick = choose(&settings("auto", None), &no_tortoise).unwrap();
    assert_eq!(pick.kind, ToolKind::Fork);

    let git_and_editor = detect(
        &Fake::windows()
            .on_path("git", r"C:\Git\cmd\git.exe")
            .file(LOCAL, r"Programs\Microsoft VS Code\Code.exe"),
    );
    assert_eq!(kinds(&git_and_editor).len(), 2);
    let pick = choose(&settings("auto", None), &git_and_editor).unwrap();
    assert_eq!(pick.kind, ToolKind::GitCli);

    let editor_only = detect(&Fake::windows().file(LOCAL, r"Programs\Microsoft VS Code\Code.exe"));
    assert_eq!(choose(&settings("auto", None), &editor_only), None);
}

#[test]
fn a_named_tool_is_chosen_when_installed_and_none_otherwise() {
    let found = detect(&all_installed());
    let pick = choose(&settings("Fork", None), &found).unwrap();
    assert_eq!(pick.kind, ToolKind::Fork);
    assert_eq!(
        choose(&settings("vscode", None), &found).unwrap().kind,
        ToolKind::VsCode
    );
    assert_eq!(
        choose(&settings("git-cli", None), &found).unwrap().kind,
        ToolKind::GitCli
    );
    assert_eq!(choose(&settings("sourcetree", None), &found), None);
    assert_eq!(choose(&settings("github-desktop", None), &found), None);
    assert_eq!(choose(&settings("no-such-tool", None), &found), None);
}

#[test]
fn none_chooses_nothing() {
    let found = detect(&all_installed());
    assert_eq!(choose(&settings("none", None), &found), None);
}

#[test]
fn custom_needs_a_command_and_ignores_detection() {
    let with = choose(&settings("custom", Some("tool {action} {path}")), &[]).unwrap();
    assert_eq!(with.kind, ToolKind::Custom);
    assert_eq!(choose(&settings("custom", None), &[]), None);
    assert_eq!(choose(&settings("custom", Some("  ")), &[]), None);
}

#[test]
fn capabilities_follow_the_open_only_rule() {
    for kind in [
        ToolKind::Fork,
        ToolKind::GitHubDesktop,
        ToolKind::SourceTree,
        ToolKind::GitKraken,
        ToolKind::SublimeMerge,
        ToolKind::VsCode,
    ] {
        assert_eq!(kind.capabilities(), [Capability::OpenOnly], "{kind:?}");
    }
    let tortoise = ToolKind::TortoiseGit.capabilities();
    for action in [
        Capability::Commit,
        Capability::Pull,
        Capability::Push,
        Capability::Log,
        Capability::Diff,
    ] {
        assert!(tortoise.contains(&action), "{action:?}");
    }
    assert!(!tortoise.contains(&Capability::OpenOnly));
    let cli = ToolKind::GitCli.capabilities();
    assert!(cli.contains(&Capability::Pull) && !cli.contains(&Capability::Diff));
}

#[test]
fn the_real_environment_detects_without_failing() {
    let found = detect(&SystemEnv);
    assert!(cfg!(windows) || found.iter().all(|tool| tool.kind == ToolKind::GitCli));
}
