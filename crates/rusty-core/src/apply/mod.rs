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

#[allow(unused_variables, reason = "stub until apply is implemented")]
pub fn apply(state: &mut AppState, command: &Command) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

#[allow(unused_variables, reason = "stub until apply is implemented")]
#[must_use]
pub fn apply_event(state: &mut AppState, event: Event) -> Vec<Effect> {
    todo!()
}

#[cfg(test)]
mod tests;
