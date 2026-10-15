use super::{CommandError, Effect, last_row, loaded_listing};
use crate::{AppState, Pattern};

pub(super) fn toggle(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    last_row(state)?;
    let row = state.active_tab().view().cursor;
    let marked = state.active_tab_mut().selection_mut().toggle(row);
    let tab = state.active_tab();
    let folder = tab
        .listing()
        .and_then(|l| l.get(row))
        .filter(|e| e.is_folder);
    Ok(match folder {
        Some(entry) if marked => vec![Effect::MeasureFolder {
            tab: tab.id(),
            location: tab.location().clone(),
            name: entry.name.clone(),
        }],
        _ => Vec::new(),
    })
}

pub(super) fn toggle_down(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    let last = last_row(state)?;
    let tab = state.active_tab_mut();
    let row = tab.view().cursor;
    tab.selection_mut().toggle(row);
    tab.set_cursor(row.saturating_add(1).min(last));
    state.focused_pane_mut().reveal_cursor();
    Ok(Vec::new())
}

pub(super) fn range(state: &mut AppState, to: u64) -> Result<Vec<Effect>, CommandError> {
    let last = last_row(state)?;
    let tab = state.active_tab_mut();
    tab.selection_mut().mark_range(to);
    tab.set_cursor(to.min(last));
    state.focused_pane_mut().reveal_cursor();
    Ok(Vec::new())
}

pub(super) fn pattern(
    state: &mut AppState,
    text: &str,
    mark: bool,
) -> Result<Vec<Effect>, CommandError> {
    let pattern = Pattern::parse(text)?;
    let listing = loaded_listing(state)?;
    let selection = state.active_tab_mut().selection_mut();
    if mark {
        selection.mark_pattern(&listing, &pattern, false);
    } else {
        selection.unmark_pattern(&listing, &pattern, false);
    }
    Ok(Vec::new())
}

pub(super) fn invert(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    let listing = loaded_listing(state)?;
    state
        .active_tab_mut()
        .selection_mut()
        .invert(&listing, false);
    Ok(Vec::new())
}

pub(super) fn all(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    let listing = loaded_listing(state)?;
    state
        .active_tab_mut()
        .selection_mut()
        .mark_all(&listing, false);
    Ok(Vec::new())
}

pub(super) fn clear(state: &mut AppState) -> Vec<Effect> {
    state.active_tab_mut().selection_mut().clear();
    Vec::new()
}
