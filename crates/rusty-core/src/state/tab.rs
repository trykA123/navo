use std::collections::{HashMap, HashSet};

use super::{History, Listing, StateError};
use crate::{Location, Selection, TabId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ViewState {
    pub cursor: u64,
    pub scroll: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    id: TabId,
    location: Location,
    view: ViewState,
    listing: Option<Listing>,
    pending: Option<u64>,
    selection: Selection,
    history: History,
    views: HashMap<Location, ViewState>,
}

impl Tab {
    #[must_use]
    pub fn id(&self) -> TabId {
        self.id
    }

    #[must_use]
    pub fn location(&self) -> &Location {
        &self.location
    }

    #[must_use]
    pub fn view(&self) -> ViewState {
        self.view
    }

    #[must_use]
    pub fn listing(&self) -> Option<&Listing> {
        self.listing.as_ref()
    }

    #[must_use]
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    #[must_use]
    pub fn history(&self) -> &History {
        &self.history
    }

    pub(crate) fn new(id: TabId, location: Location) -> Self {
        Self {
            id,
            location,
            view: ViewState::default(),
            listing: None,
            pending: None,
            selection: Selection::new(0),
            history: History::default(),
            views: HashMap::new(),
        }
    }

    pub(crate) fn restore(
        id: TabId,
        location: Location,
        history: History,
        view: ViewState,
    ) -> Self {
        Self {
            view,
            history,
            ..Self::new(id, location)
        }
    }

    pub(crate) fn navigate(&mut self, location: Location) {
        let previous = self.leave(location);
        self.history.push(previous);
        self.arrive();
    }

    pub(crate) fn back(&mut self) -> Result<(), StateError> {
        let target = self
            .history
            .back()
            .last()
            .cloned()
            .ok_or(StateError::NothingBack)?;
        let previous = self.leave(target);
        self.history.step_back(previous);
        self.arrive();
        Ok(())
    }

    pub(crate) fn forward(&mut self) -> Result<(), StateError> {
        let target = self
            .history
            .forward()
            .first()
            .cloned()
            .ok_or(StateError::NothingForward)?;
        let previous = self.leave(target);
        self.history.step_forward(previous);
        self.arrive();
        Ok(())
    }

    pub(crate) fn selection_mut(&mut self) -> &mut Selection {
        &mut self.selection
    }

    #[must_use]
    pub fn pending_request(&self) -> Option<u64> {
        self.pending
    }

    pub(crate) fn expect_listing(&mut self, request: u64) {
        self.pending = Some(request);
    }

    pub(crate) fn accept_listing(
        &mut self,
        request: u64,
        location: &Location,
        listing: Listing,
    ) -> bool {
        if self.pending != Some(request) || &self.location != location {
            return false;
        }
        self.pending = None;
        let marked: HashSet<String> = self
            .listing
            .as_ref()
            .map(|old| {
                self.selection
                    .marked()
                    .into_iter()
                    .filter_map(|row| old.get(row).map(|e| e.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let keep: Vec<u64> = (0..listing.len())
            .filter(|row| listing.get(*row).is_some_and(|e| marked.contains(&e.name)))
            .collect();
        self.set_listing(listing);
        self.selection.mark_rows(keep);
        true
    }

    pub(crate) fn set_listing(&mut self, listing: Listing) {
        self.selection = Selection::new(listing.len());
        self.listing = Some(listing);
        self.view.cursor = self.clamp(self.view.cursor);
        self.view.scroll = self.clamp(self.view.scroll);
    }

    pub(crate) fn set_cursor(&mut self, row: u64) {
        self.view.cursor = self.clamp(row);
    }

    pub(crate) fn set_scroll(&mut self, row: u64) {
        self.view.scroll = self.clamp(row);
    }

    fn leave(&mut self, target: Location) -> Location {
        self.views.insert(self.location.clone(), self.view);
        self.listing = None;
        self.pending = None;
        self.selection = Selection::new(0);
        std::mem::replace(&mut self.location, target)
    }

    fn arrive(&mut self) {
        self.view = self.views.get(&self.location).copied().unwrap_or_default();
        let history = &self.history;
        self.views.retain(|location, _| history.contains(location));
    }

    fn clamp(&self, row: u64) -> u64 {
        self.listing
            .as_ref()
            .map_or(row, |listing| row.min(listing.len().saturating_sub(1)))
    }
}
