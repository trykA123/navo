use super::ToolKind;

pub(super) struct Key {
    pub key: &'static str,
    pub value: &'static str,
    pub folder: bool,
}

/// Every place one tool may live. Any source that holds an existing file wins.
pub(super) struct Spec {
    pub kind: ToolKind,
    pub gui: bool,
    pub keys: &'static [Key],
    pub uninstall: &'static [&'static str],
    pub exes: &'static [&'static str],
    pub folders: &'static [(&'static str, &'static str)],
    pub path: &'static [&'static str],
}

const fn exe(key: &'static str, value: &'static str) -> Key {
    Key {
        key,
        value,
        folder: false,
    }
}

const fn folder(key: &'static str, value: &'static str) -> Key {
    Key {
        key,
        value,
        folder: true,
    }
}

pub(super) const UNINSTALL_ROOTS: [&str; 2] = [
    r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
    r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
];

const PF: &str = "ProgramFiles";
const PF86: &str = "ProgramFiles(x86)";
const LOCAL: &str = "LOCALAPPDATA";
const TORTOISE: &str = r"SOFTWARE\TortoiseGit";

/// In detection order, which is also the `auto` order.
pub(super) const SPECS: &[Spec] = &[
    Spec {
        kind: ToolKind::TortoiseGit,
        gui: true,
        keys: &[exe(TORTOISE, "ProcPath"), folder(TORTOISE, "Directory")],
        uninstall: &["TortoiseGit"],
        exes: &[r"bin\TortoiseGitProc.exe", "TortoiseGitProc.exe"],
        folders: &[
            (PF, r"TortoiseGit\bin\TortoiseGitProc.exe"),
            (PF86, r"TortoiseGit\bin\TortoiseGitProc.exe"),
        ],
        path: &["TortoiseGitProc"],
    },
    Spec {
        kind: ToolKind::Fork,
        gui: true,
        keys: &[exe(r"Software\Classes\fork\shell\open\command", "")],
        uninstall: &["Fork"],
        exes: &["Fork.exe"],
        folders: &[
            (LOCAL, r"Fork\Fork.exe"),
            (LOCAL, r"Fork\current\Fork.exe"),
            (LOCAL, r"Programs\Fork\Fork.exe"),
            (PF, r"Fork\Fork.exe"),
        ],
        path: &["fork"],
    },
    Spec {
        kind: ToolKind::GitHubDesktop,
        gui: true,
        keys: &[
            exe(r"Software\Classes\x-github-client\shell\open\command", ""),
            exe(r"Software\Classes\github-windows\shell\open\command", ""),
        ],
        uninstall: &["GitHubDesktop", "GitHub Desktop"],
        exes: &["GitHubDesktop.exe"],
        folders: &[
            (LOCAL, r"GitHubDesktop\GitHubDesktop.exe"),
            (PF, r"GitHub Desktop\GitHubDesktop.exe"),
            (PF86, r"GitHub Desktop\GitHubDesktop.exe"),
        ],
        path: &["GitHubDesktop"],
    },
    Spec {
        kind: ToolKind::SourceTree,
        gui: true,
        keys: &[exe(r"Software\Classes\sourcetree\shell\open\command", "")],
        uninstall: &["SourceTree", "Sourcetree"],
        exes: &["SourceTree.exe"],
        folders: &[
            (LOCAL, r"SourceTree\SourceTree.exe"),
            (LOCAL, r"Atlassian\SourceTree\SourceTree.exe"),
            (PF86, r"Atlassian\SourceTree\SourceTree.exe"),
            (PF, r"Atlassian\SourceTree\SourceTree.exe"),
        ],
        path: &["SourceTree"],
    },
    Spec {
        kind: ToolKind::GitKraken,
        gui: true,
        keys: &[exe(r"Software\Classes\gitkraken\shell\open\command", "")],
        uninstall: &["gitkraken", "GitKraken"],
        exes: &["gitkraken.exe"],
        folders: &[
            (LOCAL, r"gitkraken\gitkraken.exe"),
            (LOCAL, r"Programs\GitKraken\GitKraken.exe"),
        ],
        path: &["gitkraken"],
    },
    Spec {
        kind: ToolKind::SublimeMerge,
        gui: true,
        keys: &[],
        uninstall: &["Sublime Merge"],
        exes: &["smerge.exe", "sublime_merge.exe"],
        folders: &[
            (PF, r"Sublime Merge\smerge.exe"),
            (LOCAL, r"Programs\Sublime Merge\smerge.exe"),
        ],
        path: &["smerge"],
    },
    Spec {
        kind: ToolKind::VsCode,
        gui: true,
        keys: &[exe(r"Software\Classes\vscode\shell\open\command", "")],
        uninstall: &[],
        exes: &["Code.exe"],
        folders: &[
            (LOCAL, r"Programs\Microsoft VS Code\Code.exe"),
            (PF, r"Microsoft VS Code\Code.exe"),
            (PF86, r"Microsoft VS Code\Code.exe"),
        ],
        path: &["code"],
    },
    Spec {
        kind: ToolKind::GitCli,
        gui: false,
        keys: &[folder(r"SOFTWARE\GitForWindows", "InstallPath")],
        uninstall: &["Git_is1"],
        exes: &[r"cmd\git.exe", r"bin\git.exe"],
        folders: &[
            (PF, r"Git\cmd\git.exe"),
            (PF86, r"Git\cmd\git.exe"),
            (LOCAL, r"Programs\Git\cmd\git.exe"),
        ],
        path: &["git"],
    },
];
