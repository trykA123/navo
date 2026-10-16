mod layout;
mod pane;
mod select;
mod window;

use crate::{AppState, Command, Listing, Location, PaneId, PatternError, StateError, TabId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    ListFolder {
        tab: TabId,
        location: Location,
        request: u64,
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
        request: u64,
        listing: Listing,
    },
    ListingCached {
        tab: TabId,
        location: Location,
        request: u64,
        listing: Listing,
    },
    PageRowsChanged {
        pane: PaneId,
        rows: u64,
    },
    ListingFiltered {
        tab: TabId,
        location: Location,
        listing: Listing,
    },
    ListingRefiltered {
        tab: TabId,
        location: Location,
        listing: Listing,
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
        Command::PaneGoDrive { root } => Ok(pane::go_drive(state, root)),
        Command::PaneBack => pane::back(state),
        Command::PaneForward => pane::forward(state),
        Command::PaneRefresh => Ok(list_active(state)),
        Command::PaneCursorMove { to } => pane::cursor_move(state, *to),
        Command::PaneActivateTab { tab } => window::activate_tab(state, *tab),
        Command::PaneScroll { to } => Ok(window::scroll(state, *to)),
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
        Command::WindowOpen { location } => Ok(window::open(state, location.as_ref())),
        Command::WindowClose { window } => window::close(state, *window),
        Command::WindowFocus { window } => window::focus(state, *window),
        Command::AppSaveSession => Ok(vec![Effect::SaveSession]),
    }
}

pub fn apply_event(state: &mut AppState, event: Event) -> Vec<Effect> {
    match event {
        Event::ListingLoaded {
            tab,
            location,
            request,
            listing,
        } => {
            if let Some(tab) = state.tab_mut(tab) {
                tab.accept_listing(request, &location, listing);
            }
        }
        Event::ListingCached {
            tab,
            location,
            request,
            listing,
        } => {
            if let Some(tab) = state.tab_mut(tab) {
                tab.show_cached(request, &location, listing);
            }
        }
        Event::PageRowsChanged { pane, rows } => pane::page_rows_changed(state, pane, rows),
        Event::ListingFiltered {
            tab,
            location,
            listing,
        } => {
            if let Some(found) = state.tab_mut(tab)
                && found.replace_listing(&location, listing)
            {
                pane::reveal_tab(state, tab);
            }
        }
        Event::ListingRefiltered {
            tab,
            location,
            listing,
        } => {
            if let Some(found) = state.tab_mut(tab)
                && found.replace_listing_near(&location, listing)
            {
                pane::reveal_tab(state, tab);
            }
        }
    }
    Vec::new()
}

pub fn list_unloaded(state: &mut AppState) -> Vec<Effect> {
    state
        .unloaded_active_tabs()
        .into_iter()
        .filter_map(|tab| list_tab(state, tab))
        .collect()
}

pub fn list_active_tabs(state: &mut AppState) -> Vec<Effect> {
    state
        .active_tab_ids()
        .into_iter()
        .filter_map(|tab| list_tab(state, tab))
        .collect()
}

pub fn list_missing(state: &mut AppState) -> Vec<Effect> {
    state
        .active_tabs_without_listing()
        .into_iter()
        .filter_map(|tab| list_tab(state, tab))
        .collect()
}

pub fn relist_tab(state: &mut AppState, id: TabId) -> Vec<Effect> {
    list_tab(state, id).into_iter().collect()
}

fn list_tab(state: &mut AppState, id: TabId) -> Option<Effect> {
    let request = state.take_request();
    let tab = state.tab_mut(id)?;
    tab.expect_listing(request);
    Some(Effect::ListFolder {
        tab: id,
        location: tab.location().clone(),
        request,
    })
}

fn list_active(state: &mut AppState) -> Vec<Effect> {
    let id = state.active_tab().id();
    list_tab(state, id).into_iter().collect()
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
