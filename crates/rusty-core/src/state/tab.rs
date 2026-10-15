use std::collections::HashMap;

use super::{History, Listing, StateError};
use crate::{Location, TabId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ViewState {
    pub cursor: u64,
    pub scroll: u64,
}

#[allow(dead_code, reason = "stubs until the state is implemented")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    id: TabId,
    location: Location,
    view: ViewState,
    listing: Option<Listing>,
    history: History,
    views: HashMap<Location, ViewState>,
}

#[allow(
    unused_variables,
    dead_code,
    reason = "stubs until the state is implemented"
)]
impl Tab {
    #[must_use]
    pub fn id(&self) -> TabId {
        todo!()
    }

    #[must_use]
    pub fn location(&self) -> &Location {
        todo!()
    }

    #[must_use]
    pub fn view(&self) -> ViewState {
        todo!()
    }

    #[must_use]
    pub fn listing(&self) -> Option<&Listing> {
        todo!()
    }

    #[must_use]
    pub fn history(&self) -> &History {
        todo!()
    }

    pub(crate) fn new(id: TabId, location: Location) -> Self {
        todo!()
    }

    pub(crate) fn navigate(&mut self, location: Location) {
        todo!()
    }

    pub(crate) fn back(&mut self) -> Result<(), StateError> {
        todo!()
    }

    pub(crate) fn forward(&mut self) -> Result<(), StateError> {
        todo!()
    }

    pub(crate) fn set_listing(&mut self, listing: Listing) {
        todo!()
    }

    pub(crate) fn set_cursor(&mut self, row: u64) {
        todo!()
    }

    pub(crate) fn set_scroll(&mut self, row: u64) {
        todo!()
    }
}
