#![allow(
    dead_code,
    unused_variables,
    reason = "stubs until apply is implemented"
)]

use super::{CommandError, Effect};
use crate::{AppState, CursorMove, Listing, Location, PaneId, TabId};

pub(super) fn open_tab(
    state: &mut AppState,
    location: Option<&Location>,
) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn close_tab(
    state: &mut AppState,
    tab: Option<u32>,
) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn navigate(
    state: &mut AppState,
    location: &Location,
) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn back(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn forward(state: &mut AppState) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn cursor_move(
    state: &mut AppState,
    to: CursorMove,
) -> Result<Vec<Effect>, CommandError> {
    todo!()
}

pub(super) fn listing_loaded(
    state: &mut AppState,
    tab: TabId,
    location: &Location,
    listing: Listing,
) -> Vec<Effect> {
    todo!()
}

pub(super) fn page_rows_changed(state: &mut AppState, pane: PaneId, rows: u64) -> Vec<Effect> {
    todo!()
}
