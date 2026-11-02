use super::{CommandError, Effect, apply_workspace};
use crate::{AppState, Command, WorkspaceAction};

pub(super) fn apply(state: &mut AppState, command: &Command) -> Result<Vec<Effect>, CommandError> {
    let action = match command {
        Command::WorkspaceSwitch { n } => WorkspaceAction::Switch { n: *n },
        Command::WorkspaceNew { name } => WorkspaceAction::New { name: name.clone() },
        Command::WorkspaceRename { n, name } => WorkspaceAction::Rename {
            n: *n,
            name: name.clone(),
        },
        Command::WorkspaceMove { n, by } => WorkspaceAction::Move { n: *n, by: *by },
        Command::WorkspaceDelete { n } => WorkspaceAction::Delete { n: *n },
        Command::WorkspaceRestore => WorkspaceAction::Restore,
        Command::WorkspaceSave => WorkspaceAction::Save,
        Command::WorkspaceDiscard { n } => WorkspaceAction::Discard { n: *n },
        _ => return Ok(Vec::new()),
    };
    Ok(apply_workspace(state, &action)?)
}
