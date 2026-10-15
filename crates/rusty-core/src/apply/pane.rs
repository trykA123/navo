use super::{CommandError, Effect, last_row, list_active};
use crate::{AppState, CursorMove, Location, PaneId};

pub(super) fn open_tab(state: &mut AppState, location: Option<&Location>) -> Vec<Effect> {
    let location = location
        .cloned()
        .unwrap_or_else(|| state.active_tab().location().clone());
    let tab = state.new_tab(location);
    state.focused_pane_mut().open_tab(tab);
    list_active(state)
}

pub(super) fn close_tab(
    state: &mut AppState,
    tab: Option<u32>,
) -> Result<Vec<Effect>, CommandError> {
    let pane = state.focused_pane_mut();
    let index = tab.map_or(pane.active_index(), |t| {
        usize::try_from(t).unwrap_or(usize::MAX)
    });
    pane.close_tab(index)?;
    Ok(Vec::new())
}

pub(super) fn navigate(state: &mut AppState, location: &Location) -> Vec<Effect> {
    state.active_tab_mut().navigate(location.clone());
    list_active(state)
}

pub(super) fn back(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    state.active_tab_mut().back()?;
    Ok(list_active(state))
}

pub(super) fn forward(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    state.active_tab_mut().forward()?;
    Ok(list_active(state))
}

pub(super) fn cursor_move(
    state: &mut AppState,
    to: CursorMove,
) -> Result<Vec<Effect>, CommandError> {
    let last = last_row(state)?;
    let cursor = state.active_tab().view().cursor;
    let page = state
        .focused_pane_mut()
        .page_rows()
        .saturating_sub(1)
        .max(1);
    let row = match to {
        CursorMove::Up => cursor.saturating_sub(1),
        CursorMove::Down => cursor.saturating_add(1),
        CursorMove::PageUp => cursor.saturating_sub(page),
        CursorMove::PageDown => cursor.saturating_add(page),
        CursorMove::First => 0,
        CursorMove::Last => last,
        CursorMove::Row(row) => row,
    };
    state.active_tab_mut().set_cursor(row.min(last));
    Ok(Vec::new())
}

pub(super) fn page_rows_changed(state: &mut AppState, pane: PaneId, rows: u64) {
    if let Some(pane) = state.pane_mut(pane) {
        pane.set_page_rows(rows);
    }
}
