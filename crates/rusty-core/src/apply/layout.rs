use super::{CommandError, Effect, list_active};
use crate::{AppState, Direction};

pub(super) fn split(
    state: &mut AppState,
    direction: Direction,
) -> Result<Vec<Effect>, CommandError> {
    state.split(direction)?;
    Ok(list_active(state))
}

pub(super) fn close(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    let pane = state.focused_pane();
    state.close_pane(pane)?;
    Ok(Vec::new())
}

pub(super) fn focus(
    state: &mut AppState,
    direction: Direction,
) -> Result<Vec<Effect>, CommandError> {
    state.focus_direction(direction)?;
    Ok(Vec::new())
}
