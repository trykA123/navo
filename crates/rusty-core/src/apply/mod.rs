mod layout;
mod pane;
mod select;

use crate::{AppState, Command, Listing, Location, PaneId, PatternError, StateError, TabId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    ListFolder {
        tab: TabId,
        location: Location,
    },
    MeasureFolder {
        tab: TabId,
        location: Location,
        name: String,
    },
    SaveSession,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    ListingLoaded {
        tab: TabId,
        location: Location,
        listing: Listing,
    },
    PageRowsChanged {
        pane: PaneId,
        rows: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CommandError {
    #[error(transparent)]
    State(#[from] StateError),
    #[error(transparent)]
    Pattern(#[from] PatternError),
    #[error("the folder is not loaded yet")]
    NoListing,
    #[error("the folder is empty")]
    EmptyListing,
}

pub fn apply(state: &mut AppState, command: &Command) -> Result<Vec<Effect>, CommandError> {
    match command {
        Command::PaneOpenTab { location } => Ok(pane::open_tab(state, location.as_ref())),
        Command::PaneCloseTab { tab } => pane::close_tab(state, *tab),
        Command::PaneNavigate { location } => Ok(pane::navigate(state, location)),
        Command::PaneBack => pane::back(state),
        Command::PaneForward => pane::forward(state),
        Command::PaneCursorMove { to } => pane::cursor_move(state, *to),
        Command::SelectionToggle => select::toggle(state),
        Command::SelectionToggleDown => select::toggle_down(state),
        Command::SelectionRange { to } => select::range(state, *to),
        Command::SelectionAddPattern { pattern } => select::pattern(state, pattern, true),
        Command::SelectionRemovePattern { pattern } => select::pattern(state, pattern, false),
        Command::SelectionInvert => select::invert(state),
        Command::SelectionAll => select::all(state),
        Command::SelectionClear => Ok(select::clear(state)),
        Command::LayoutSplit { direction } => layout::split(state, *direction),
        Command::LayoutClose => layout::close(state),
        Command::LayoutFocus { direction } => layout::focus(state, *direction),
        Command::AppSaveSession => Ok(vec![Effect::SaveSession]),
    }
}

pub fn apply_event(state: &mut AppState, event: Event) -> Vec<Effect> {
    match event {
        Event::ListingLoaded {
            tab,
            location,
            listing,
        } => pane::listing_loaded(state, tab, &location, listing),
        Event::PageRowsChanged { pane, rows } => pane::page_rows_changed(state, pane, rows),
    }
    Vec::new()
}

fn list_active(state: &AppState) -> Vec<Effect> {
    let tab = state.active_tab();
    vec![Effect::ListFolder {
        tab: tab.id(),
        location: tab.location().clone(),
    }]
}

fn loaded_listing(state: &AppState) -> Result<Listing, CommandError> {
    state
        .active_tab()
        .listing()
        .cloned()
        .ok_or(CommandError::NoListing)
}

fn last_row(state: &AppState) -> Result<u64, CommandError> {
    loaded_listing(state)?
        .len()
        .checked_sub(1)
        .ok_or(CommandError::EmptyListing)
}

#[cfg(test)]
mod tests;
