use std::path::PathBuf;

use navo_ui::git::GitAction;

use super::tools::{Capability, DetectedTool, ToolKind};

mod custom;
pub(crate) mod start;

/// One program argument. `Raw` reaches the Windows command line as written, with no quoting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Arg {
    Plain(String),
    Raw(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Process {
    pub program: PathBuf,
    pub args: Vec<Arg>,
    pub console: bool,
}

/// What to start. Built without any shell; starting it is `start`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Launch {
    Process(Process),
    Terminal { repo: String },
    Url(String),
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum LaunchError {
    #[error("the repository path cannot be passed to a tool")]
    BadPath,
    #[error("{0} cannot do this action")]
    Unsupported(String),
    #[error("the custom Git command is empty")]
    EmptyCommand,
    #[error("the custom Git command needs a {{path}} argument")]
    MissingPath,
    #[error("{{path}} must stand alone as an argument and never follow a shell")]
    PathPlacement,
    #[error("the program of the custom Git command cannot hold a placeholder")]
    ProgramPlaceholder,
    #[error("{0}")]
    Command(String),
    #[error("could not start the tool: {0}")]
    Spawn(String),
    #[error("{0}")]
    Terminal(String),
}

/// Why `template` cannot be the command of the Other tool; builds a launch and starts nothing.
pub(crate) fn check_template(template: &str) -> Result<(), LaunchError> {
    custom::expand(Some(template), "commit", "repo").map(drop)
}

/// Builds the launch for `action` on `repo`. `template` is `[git] tool-command`, used by `Custom` only.
pub(crate) fn plan(
    tool: &DetectedTool,
    action: GitAction,
    repo: &str,
    template: Option<&str>,
) -> Result<Launch, LaunchError> {
    check_repo(repo)?;
    let kind = tool.kind;
    let unsupported = || LaunchError::Unsupported(kind.label());
    if !allows(kind, action) {
        return Err(unsupported());
    }
    match kind {
        ToolKind::GitCli => Ok(Launch::Terminal {
            repo: repo.to_owned(),
        }),
        ToolKind::Custom => {
            let name = action_name(action).ok_or_else(unsupported)?;
            custom::expand(template, name, repo).map(Launch::Process)
        }
        ToolKind::TortoiseGit => {
            let command = tortoise_command(action).ok_or_else(unsupported)?;
            let args = vec![
                Arg::Plain(format!("/command:{command}")),
                Arg::Raw(format!("/path:\"{}\"", tortoise_path(repo))),
            ];
            Ok(process(tool, args, false))
        }
        _ => Ok(process(tool, open_args(kind, repo), has_console(kind))),
    }
}

fn process(tool: &DetectedTool, args: Vec<Arg>, console: bool) -> Launch {
    Launch::Process(Process {
        program: tool.exe.clone(),
        args,
        console,
    })
}

fn check_repo(repo: &str) -> Result<(), LaunchError> {
    let bad = repo.is_empty() || repo.starts_with('-') || repo.chars().any(|ch| ch.is_control());
    if bad || repo.contains('"') {
        return Err(LaunchError::BadPath);
    }
    Ok(())
}

fn allows(kind: ToolKind, action: GitAction) -> bool {
    let wanted = match action {
        GitAction::Commit => Capability::Commit,
        GitAction::Pull => Capability::Pull,
        GitAction::Push => Capability::Push,
        GitAction::Log => Capability::Log,
        GitAction::Diff => Capability::Diff,
        GitAction::Open => Capability::OpenOnly,
        GitAction::ChooseTool => return false,
    };
    let remote = matches!(action, GitAction::Pull | GitAction::Push);
    kind.capabilities().contains(&wanted) && !(kind == ToolKind::GitCli && remote)
}

fn action_name(action: GitAction) -> Option<&'static str> {
    Some(match action {
        GitAction::Commit => "commit",
        GitAction::Pull => "pull",
        GitAction::Push => "push",
        GitAction::Log => "log",
        GitAction::Diff => "diff",
        GitAction::Open | GitAction::ChooseTool => return None,
    })
}

fn tortoise_command(action: GitAction) -> Option<&'static str> {
    match action {
        GitAction::Diff => Some("repostatus"),
        other => action_name(other),
    }
}

/// A trailing separator before the closing quote would escape it; a drive root keeps a `.`.
fn tortoise_path(repo: &str) -> String {
    let trimmed = repo.trim_end_matches(['\\', '/']);
    if trimmed.is_empty() || trimmed.ends_with(':') {
        format!("{trimmed}\\.")
    } else {
        trimmed.to_owned()
    }
}

fn open_args(kind: ToolKind, repo: &str) -> Vec<Arg> {
    let plain = |text: &str| Arg::Plain(text.to_owned());
    match kind {
        ToolKind::GitHubDesktop => vec![plain("--cli-open"), plain(repo)],
        ToolKind::SourceTree => vec![plain("-f"), plain(repo)],
        _ => vec![plain(repo)],
    }
}

fn has_console(kind: ToolKind) -> bool {
    matches!(kind, ToolKind::SublimeMerge | ToolKind::VsCode)
}

#[cfg(test)]
mod tests;
