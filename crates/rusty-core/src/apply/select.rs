use super::{CommandError, Effect, last_row, loaded_listing};
use crate::{AppState, CursorMove, Pattern, SpanMode, TabId};

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

pub(super) fn extend(
    state: &mut AppState,
    to: CursorMove,
    mode: SpanMode,
) -> Result<Vec<Effect>, CommandError> {
    let target = super::pane::cursor_target(state, to)?;
    let span_to = match to {
        CursorMove::Row(row) => row,
        _ => target,
    };
    let tab = state.active_tab_mut();
    let cursor = tab.view().cursor;
    tab.selection_mut().extend(cursor, span_to, mode);
    tab.set_cursor(target);
    state.focused_pane_mut().reveal_cursor();
    Ok(Vec::new())
}

pub(super) fn extension(state: &mut AppState, mark: bool) -> Result<Vec<Effect>, CommandError> {
    let listing = loaded_listing(state)?;
    let entry = listing
        .get(state.active_tab().view().cursor)
        .ok_or(CommandError::EmptyListing)?;
    if entry.is_folder {
        return Ok(Vec::new());
    }
    state.active_tab_mut().selection_mut().mark_extension(
        &listing,
        crate::selection::extension(&entry.name),
        mark,
    );
    Ok(Vec::new())
}

pub(super) fn restore(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    let listing = loaded_listing(state)?;
    let names: std::collections::HashSet<&str> = state
        .last_marks()
        .ok_or(CommandError::NothingToRestore)?
        .iter()
        .map(String::as_str)
        .collect();
    let rows: Vec<u64> = listing
        .entries()
        .iter()
        .zip(0..)
        .filter_map(|(entry, row)| names.contains(entry.name.as_str()).then_some(row))
        .collect();
    if rows.is_empty() {
        return Err(CommandError::NothingToRestore);
    }
    state.active_tab_mut().selection_mut().mark_rows(rows);
    Ok(Vec::new())
}

pub(super) struct Marking {
    pub mark: bool,
    pub folders: bool,
}

pub(super) fn pattern(
    state: &mut AppState,
    text: &str,
    marking: Marking,
) -> Result<Vec<Effect>, CommandError> {
    let pattern = Pattern::parse(text)?;
    let listing = loaded_listing(state)?;
    let selection = state.active_tab_mut().selection_mut();
    if marking.mark {
        selection.mark_pattern(&listing, &pattern, marking.folders);
    } else {
        selection.unmark_pattern(&listing, &pattern, marking.folders);
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
        .mark_all(&listing, true);
    Ok(Vec::new())
}

pub(super) fn clear(state: &mut AppState) -> Vec<Effect> {
    state.remember_marks(state.active_tab().marked_names());
    state.active_tab_mut().selection_mut().clear();
    Vec::new()
}

pub(super) fn mark_names(state: &mut AppState, tab: TabId, names: &[String]) {
    let Some(tab) = state.tab_mut(tab) else {
        return;
    };
    let wanted: std::collections::HashSet<&str> = names.iter().map(String::as_str).collect();
    let rows: Vec<u64> = tab
        .listing()
        .into_iter()
        .flat_map(|listing| listing.entries().iter().zip(0..))
        .filter_map(|(entry, row)| wanted.contains(entry.name.as_str()).then_some(row))
        .collect();
    let selection = tab.selection_mut();
    selection.mark_rows(rows);
    selection.end_span();
}
