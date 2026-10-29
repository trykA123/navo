use super::{CommandError, Effect, list_unloaded};
use crate::{AppState, TabCommand};

pub(super) fn apply(
    state: &mut AppState,
    command: &TabCommand,
) -> Result<Vec<Effect>, CommandError> {
    let tab = command.tab().unwrap_or_else(|| state.active_tab().id());
    match command {
        TabCommand::Pin { pinned, .. } => state.pin_tab(tab, *pinned)?,
        TabCommand::Rename { label, .. } => state.rename_tab(tab, label)?,
        TabCommand::Duplicate { .. } => state.duplicate_tab(tab).map(drop)?,
        TabCommand::Close { .. } => state.close_tab(tab)?,
        TabCommand::CloseOthers { .. } => state.close_other_tabs(tab)?,
        TabCommand::CloseRight { .. } => state.close_tabs_right(tab)?,
        TabCommand::MoveToWorkspace { n, .. } => state.move_tab_to_workspace(tab, *n)?,
        TabCommand::MoveToNewWorkspace { .. } => state.move_tab_to_new_workspace(tab)?,
        TabCommand::MoveToPane { n, .. } => state.move_tab_to_pane(tab, *n)?,
        TabCommand::MoveToNewPane { .. } => state.move_tab_to_new_pane(tab)?,
        TabCommand::Reopen => state.reopen_tab().map(drop)?,
    }
    let mut effects = list_unloaded(state);
    effects.push(Effect::SaveSession);
    Ok(effects)
}
