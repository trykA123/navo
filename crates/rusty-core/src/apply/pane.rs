use super::{CommandError, Effect, last_row, list_active};
use crate::{AppState, CursorMove, Location, PaneId, TabId};

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
    let marks = state.active_tab_mut().navigate(location.clone());
    state.remember_marks(marks);
    list_active(state)
}

pub(super) fn go_drive(state: &mut AppState, root: &Location) -> Vec<Effect> {
    let tab = state.active_tab();
    let target = if tab.location().root().same_place(root) {
        root.clone()
    } else {
        let history = tab.history();
        history
            .back()
            .iter()
            .rev()
            .chain(history.forward())
            .find(|visited| visited.root().same_place(root))
            .cloned()
            .unwrap_or_else(|| root.clone())
    };
    navigate(state, &target)
}

pub(super) fn back(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    let marks = state.active_tab_mut().back()?;
    state.remember_marks(marks);
    Ok(list_active(state))
}

pub(super) fn forward(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    let marks = state.active_tab_mut().forward()?;
    state.remember_marks(marks);
    Ok(list_active(state))
}

pub(super) fn cursor_move(
    state: &mut AppState,
    to: CursorMove,
) -> Result<Vec<Effect>, CommandError> {
    let row = cursor_target(state, to)?;
    state.active_tab_mut().set_cursor(row);
    state.focused_pane_mut().reveal_cursor();
    Ok(Vec::new())
}

pub(super) fn cursor_target(state: &AppState, to: CursorMove) -> Result<u64, CommandError> {
    let last = last_row(state)?;
    let cursor = state.active_tab().view().cursor;
    let page = state
        .pane(state.focused_pane())
        .map_or(1, crate::Pane::page_rows)
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
    Ok(row.min(last))
}

pub(super) fn reveal_tab(state: &mut AppState, tab: TabId) {
    let panes: Vec<PaneId> = state
        .window_ids()
        .into_iter()
        .filter_map(|window| state.window(window))
        .flat_map(|window| window.panes())
        .filter(|pane| pane.active().id() == tab)
        .map(|pane| pane.id())
        .collect();
    for pane in panes {
        if let Some(pane) = state.pane_mut(pane) {
            pane.reveal_cursor();
        }
    }
}

pub(super) fn page_rows_changed(state: &mut AppState, pane: PaneId, rows: u64) {
    if let Some(pane) = state.pane_mut(pane) {
        pane.set_page_rows(rows);
        pane.reveal_cursor();
    }
}
