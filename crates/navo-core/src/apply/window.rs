use super::{CommandError, Effect, list_active, list_unloaded};
use crate::{AppState, Location, WindowId};

pub(super) fn open(state: &mut AppState, location: Option<&Location>) -> Vec<Effect> {
    let location = location
        .cloned()
        .unwrap_or_else(|| state.active_tab().location().clone());
    state.open_window(location);
    list_active(state)
}

pub(super) fn close(
    state: &mut AppState,
    window: Option<u64>,
) -> Result<Vec<Effect>, CommandError> {
    let id = window.map_or_else(|| state.focused_window(), WindowId::new);
    state.close_window(id)?;
    Ok(list_unloaded(state))
}

pub(super) fn focus(state: &mut AppState, window: u64) -> Result<Vec<Effect>, CommandError> {
    state.focus_window(WindowId::new(window))?;
    Ok(list_unloaded(state))
}

pub(super) fn activate_tab(state: &mut AppState, tab: u32) -> Result<Vec<Effect>, CommandError> {
    let index = usize::try_from(tab).unwrap_or(usize::MAX);
    state.focused_pane_mut().activate(index)?;
    Ok(list_unloaded(state))
}

pub(super) fn scroll(state: &mut AppState, to: u64) -> Vec<Effect> {
    state.active_tab_mut().set_scroll(to);
    Vec::new()
}
