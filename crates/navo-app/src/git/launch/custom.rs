use std::path::PathBuf;

use super::{Arg, LaunchError, Process};
use crate::terminal::{is_shell_program, program_identity, split};

/// Expands `[git] tool-command`. `{path}` is a whole argument that never follows a shell;
/// `{action}` may sit inside an argument. Nothing runs through a shell.
pub(super) fn expand(
    template: Option<&str>,
    action: &str,
    repo: &str,
) -> Result<Process, LaunchError> {
    let mut words = split(template.unwrap_or_default())
        .map_err(LaunchError::Command)?
        .into_iter();
    let program = words
        .next()
        .filter(|word| !word.is_empty())
        .ok_or(LaunchError::EmptyCommand)?;
    if program.contains("{path}") || program.contains("{action}") {
        return Err(LaunchError::ProgramPlaceholder);
    }
    let repo = if program_identity(&program) == "wt" {
        repo.replace(';', r"\;")
    } else {
        repo.to_owned()
    };
    let mut shell = is_shell_program(&program);
    let mut args = Vec::new();
    let mut has_path = false;
    for word in words {
        if word.contains("{path}") && (word != "{path}" || shell) {
            return Err(LaunchError::PathPlacement);
        }
        has_path |= word == "{path}";
        shell |= is_shell_program(&word);
        args.push(Arg::Plain(if word == "{path}" {
            repo.clone()
        } else {
            word.replace("{action}", action)
        }));
    }
    if !has_path {
        return Err(LaunchError::MissingPath);
    }
    Ok(Process {
        program: PathBuf::from(program),
        args,
        console: true,
    })
}
