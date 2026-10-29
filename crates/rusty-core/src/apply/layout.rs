use super::{CommandError, Effect, list_active, list_missing};
use crate::{AppState, Command, Direction, StateError};

pub(super) fn split(
    state: &mut AppState,
    direction: Direction,
) -> Result<Vec<Effect>, CommandError> {
    state.split(direction)?;
    Ok(list_missing(state))
}

pub(super) fn close(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    let pane = state.focused_pane();
    state.close_pane(pane)?;
    Ok(list_focused_missing(state, pane))
}

pub(super) fn close_discarding_tabs(
    state: &mut AppState,
    pane: crate::PaneId,
) -> Result<Vec<Effect>, CommandError> {
    state.close_pane_discarding_tabs(pane)?;
    Ok(list_focused_missing(state, pane))
}

pub(super) fn focus(
    state: &mut AppState,
    direction: Direction,
) -> Result<Vec<Effect>, CommandError> {
    let before = state.focused_pane();
    state.focus_direction(direction)?;
    Ok(list_focused_missing(state, before))
}

pub(super) fn apply(state: &mut AppState, command: &Command) -> Result<Vec<Effect>, CommandError> {
    let before = state.focused_pane();
    match command {
        Command::LayoutSplit { direction } => return split(state, *direction),
        Command::LayoutClose => return close(state),
        Command::LayoutFocus { direction } => return focus(state, *direction),
        Command::LayoutPreset { tiles } => {
            state.apply_tiles(tiles);
            return Ok(list_missing(state));
        }
        Command::LayoutSwap { a, b } => {
            let ids = state
                .window(state.focused_window())
                .map(|w| w.layout().numbered())
                .unwrap_or_default();
            let pane = |n: usize| {
                n.checked_sub(1)
                    .and_then(|i| ids.get(i))
                    .copied()
                    .ok_or(StateError::NoSuchPane(n))
            };
            state.swap(pane(*a)?, pane(*b)?)?;
        }
        Command::LayoutSwapToward { direction } => state.swap_toward(*direction)?,
        Command::LayoutResize { direction } => state.resize(*direction, 50)?,
        Command::LayoutRatio { split, ratio } => state.set_ratio(split, *ratio)?,
        Command::LayoutEven => state.even(),
        Command::LayoutFocusPane { n } => state.focus_number(*n)?,
        _ => unreachable!("dispatch sends only layout commands here"),
    }
    Ok(list_focused_missing(state, before))
}

fn list_focused_missing(state: &mut AppState, before: crate::PaneId) -> Vec<Effect> {
    if state.focused_pane() != before && state.active_tab().listing().is_none() {
        list_active(state)
    } else {
        Vec::new()
    }
}
