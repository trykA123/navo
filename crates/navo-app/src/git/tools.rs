use std::path::PathBuf;

use navo_l10n::tr;
use navo_store::GitSettings;

pub(crate) mod env;
mod find;
mod specs;

use env::ToolEnv;

/// A Git tool Navo can hand a repository to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ToolKind {
    TortoiseGit,
    Fork,
    GitHubDesktop,
    SourceTree,
    GitKraken,
    SublimeMerge,
    VsCode,
    GitCli,
    Custom,
}

/// What a tool can do for a repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Capability {
    OpenOnly,
    Commit,
    Pull,
    Push,
    Log,
    Diff,
}

const AUTO_ORDER: [ToolKind; 5] = [
    ToolKind::TortoiseGit,
    ToolKind::Fork,
    ToolKind::GitHubDesktop,
    ToolKind::SourceTree,
    ToolKind::GitKraken,
];
const FULL: [Capability; 5] = [
    Capability::Commit,
    Capability::Pull,
    Capability::Push,
    Capability::Log,
    Capability::Diff,
];
const CLI: [Capability; 4] = [
    Capability::Pull,
    Capability::Push,
    Capability::Commit,
    Capability::Log,
];

impl ToolKind {
    pub(crate) fn capabilities(self) -> &'static [Capability] {
        match self {
            Self::TortoiseGit | Self::Custom => &FULL,
            Self::GitCli => &CLI,
            _ => &[Capability::OpenOnly],
        }
    }

    pub(crate) fn label(self) -> String {
        tr(match self {
            Self::TortoiseGit => "git-tool-tortoise-git",
            Self::Fork => "git-tool-fork",
            Self::GitHubDesktop => "git-tool-github-desktop",
            Self::SourceTree => "git-tool-source-tree",
            Self::GitKraken => "git-tool-git-kraken",
            Self::SublimeMerge => "git-tool-sublime-merge",
            Self::VsCode => "git-tool-vs-code",
            Self::GitCli => "git-tool-git-command-line",
            Self::Custom => "git-tool-custom",
        })
    }

    /// The `[git] tool` value that names this tool.
    pub(crate) fn id(self) -> &'static str {
        match self {
            Self::TortoiseGit => "tortoisegit",
            Self::Fork => "fork",
            Self::GitHubDesktop => "github-desktop",
            Self::SourceTree => "sourcetree",
            Self::GitKraken => "gitkraken",
            Self::SublimeMerge => "sublime-merge",
            Self::VsCode => "vscode",
            Self::GitCli => "git-cli",
            Self::Custom => "custom",
        }
    }

    fn from_setting(name: &str) -> Option<Self> {
        Some(match name {
            "tortoisegit" => Self::TortoiseGit,
            "fork" => Self::Fork,
            "github-desktop" => Self::GitHubDesktop,
            "sourcetree" => Self::SourceTree,
            "gitkraken" => Self::GitKraken,
            "sublime-merge" => Self::SublimeMerge,
            "vscode" => Self::VsCode,
            "git-cli" => Self::GitCli,
            _ => return None,
        })
    }

    fn auto_rank(self) -> Option<usize> {
        AUTO_ORDER.iter().position(|kind| *kind == self)
    }
}

/// An installed tool. `exe` is empty for `Custom`, which runs the settings template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DetectedTool {
    pub kind: ToolKind,
    pub label: String,
    pub exe: PathBuf,
}

/// Finds installed tools in detection order. Reads only; spawns nothing.
pub(crate) fn detect(env: &impl ToolEnv) -> Vec<DetectedTool> {
    specs::SPECS
        .iter()
        .filter(|spec| spec.gui <= env.windows())
        .filter_map(|spec| {
            let exe = find::first_existing(spec, env)?;
            Some(DetectedTool {
                kind: spec.kind,
                label: spec.kind.label(),
                exe,
            })
        })
        .collect()
}

/// Applies `[git] tool`. `auto` takes the first GUI tool of the D100h order, else `git-cli`.
pub(crate) fn choose(settings: &GitSettings, detected: &[DetectedTool]) -> Option<DetectedTool> {
    let name = settings.tool.trim().to_ascii_lowercase();
    match name.as_str() {
        "auto" => detected
            .iter()
            .filter_map(|tool| Some((tool.kind.auto_rank()?, tool)))
            .min_by_key(|(rank, _)| *rank)
            .map(|(_, tool)| tool)
            .or_else(|| detected.iter().find(|tool| tool.kind == ToolKind::GitCli))
            .cloned(),
        "custom" => custom(settings),
        _ => {
            let kind = ToolKind::from_setting(&name)?;
            detected.iter().find(|tool| tool.kind == kind).cloned()
        }
    }
}

fn custom(settings: &GitSettings) -> Option<DetectedTool> {
    let command = settings.tool_command.as_deref()?;
    (!command.trim().is_empty()).then(|| DetectedTool {
        kind: ToolKind::Custom,
        label: ToolKind::Custom.label(),
        exe: PathBuf::new(),
    })
}

#[cfg(test)]
mod tests;
