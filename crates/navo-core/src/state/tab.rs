use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::{AppState, History, Listing, StateError};
use crate::{Location, LocationError, PaneId, ProviderPath, Selection, TabId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ViewState {
    pub cursor: u64,
    pub scroll: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub(super) id: TabId,
    location: Location,
    base: Option<Location>,
    view: ViewState,
    listing: Option<Listing>,
    pending: Option<u64>,
    selection: Arc<Selection>,
    history: History,
    views: HashMap<Location, ViewState>,
    names: HashMap<Location, String>,
    restore: Option<String>,
    pub(super) pinned: bool,
    pub(super) label: Option<String>,
}

impl AppState {
    pub fn open_results_tab(&mut self, pane: PaneId, base: Location) -> Result<TabId, StateError> {
        if self.pane(pane).is_none() {
            return Err(StateError::UnknownPane(pane));
        }
        let id = TabId::new(self.take_ids(1));
        #[expect(
            clippy::expect_used,
            reason = "a fixed provider and numeric id form a valid URL"
        )]
        let location = Location::new(format!("find://{}", id.get()))
            .expect("a numeric find URL is a valid location");
        let mut tab = Tab::new(id, location);
        tab.set_base(Some(base));
        tab.set_listing(Listing::new(Vec::new()));
        self.pane_mut(pane)
            .ok_or(StateError::UnknownPane(pane))?
            .open_tab(tab);
        Ok(id)
    }
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
    pub fn base_or_location(&self) -> &Location {
        self.base.as_ref().unwrap_or(&self.location)
    }

    pub fn set_base(&mut self, base: Option<Location>) {
        self.base = base;
    }

    pub fn entry_location(&self, name: &str) -> Result<Location, LocationError> {
        let Some(base) = &self.base else {
            return self.location.join(&ProviderPath::from_text(name));
        };
        let drive_rooted = matches!(name.as_bytes(), [drive, b':', b'\\' | b'/', ..] if drive.is_ascii_alphabetic());
        if drive_rooted || name.starts_with(r"\\") || name.starts_with('/') {
            return Location::new(name);
        }
        name.split(['\\', '/'])
            .try_fold(base.clone(), |location, segment| {
                location.join(&ProviderPath::from_text(segment))
            })
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
    pub fn selection_arc(&self) -> &Arc<Selection> {
        &self.selection
    }

    #[must_use]
    pub fn history(&self) -> &History {
        &self.history
    }

    #[must_use]
    pub fn is_pinned(&self) -> bool {
        self.pinned
    }

    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub(super) fn workspace_snapshot(&self) -> Self {
        Self {
            view: self.view,
            history: self.history.clone(),
            views: self.views.clone(),
            names: self.names.clone(),
            restore: self.restore.clone(),
            pinned: self.pinned,
            label: self.label.clone(),
            ..Self::new(self.id, self.location.clone())
        }
    }

    pub(crate) fn new(id: TabId, location: Location) -> Self {
        Self {
            id,
            location,
            base: None,
            view: ViewState::default(),
            listing: None,
            pending: None,
            selection: Arc::new(Selection::new(0)),
            history: History::default(),
            views: HashMap::new(),
            names: HashMap::new(),
            restore: None,
            pinned: false,
            label: None,
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

    pub(crate) fn navigate(&mut self, location: Location) -> Vec<String> {
        let (previous, marks) = self.leave(location);
        self.history.push(previous);
        self.arrive();
        marks
    }

    pub(crate) fn back(&mut self) -> Result<Vec<String>, StateError> {
        let target = self
            .history
            .back()
            .last()
            .cloned()
            .ok_or(StateError::NothingBack)?;
        let (previous, marks) = self.leave(target);
        self.history.step_back(previous);
        self.arrive();
        Ok(marks)
    }

    pub(crate) fn forward(&mut self) -> Result<Vec<String>, StateError> {
        let target = self
            .history
            .forward()
            .first()
            .cloned()
            .ok_or(StateError::NothingForward)?;
        let (previous, marks) = self.leave(target);
        self.history.step_forward(previous);
        self.arrive();
        Ok(marks)
    }

    #[must_use]
    pub fn marked_names(&self) -> Vec<String> {
        self.selection
            .marked()
            .into_iter()
            .filter_map(|row| self.listing.as_ref()?.get(row))
            .take(super::window::MAX_LAST_MARKS)
            .map(|entry| entry.name.clone())
            .collect()
    }

    pub(crate) fn selection_mut(&mut self) -> &mut Selection {
        Arc::make_mut(&mut self.selection)
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
        let followed = self
            .listing
            .as_ref()
            .map(|old| old.follow(self.view.cursor, &listing));
        match followed {
            Some(row) => {
                let offset = self.cursor_offset();
                self.keep_by_name(listing);
                self.place_cursor(row, offset);
            }
            None => self.arrive_listing(listing),
        }
        true
    }

    pub(crate) fn show_cached(
        &mut self,
        request: u64,
        location: &Location,
        listing: Listing,
    ) -> bool {
        if self.pending != Some(request) || &self.location != location || self.listing.is_some() {
            return false;
        }
        self.arrive_listing(listing);
        true
    }

    fn arrive_listing(&mut self, listing: Listing) {
        let offset = self.cursor_offset();
        let row = self.restore.take().and_then(|name| listing.position(&name));
        self.set_listing(listing);
        if let Some(row) = row {
            self.place_cursor(row, offset);
        }
    }

    fn cursor_offset(&self) -> u64 {
        self.view.cursor.saturating_sub(self.view.scroll)
    }

    fn place_cursor(&mut self, row: u64, offset: u64) {
        self.view.cursor = self.clamp(row);
        self.view.scroll = self.clamp(self.view.cursor.saturating_sub(offset));
    }

    pub(crate) fn replace_listing(&mut self, location: &Location, listing: Listing) -> bool {
        if &self.location != location || self.listing.is_none() {
            return false;
        }
        let current = self
            .listing
            .as_ref()
            .and_then(|old| old.get(self.view.cursor))
            .map(|entry| entry.name.clone());
        let cursor = current
            .and_then(|name| listing.entries().iter().position(|e| e.name == name))
            .unwrap_or(0);
        self.keep_by_name(listing);
        self.view.cursor = cursor as u64;
        self.view.scroll = self.clamp(self.view.scroll.min(self.view.cursor));
        true
    }

    pub(crate) fn replace_listing_near(&mut self, location: &Location, listing: Listing) -> bool {
        let Some(old) = self.listing.as_ref().filter(|_| &self.location == location) else {
            return false;
        };
        let names: Vec<&str> = old.entries().iter().map(|e| e.name.as_str()).collect();
        let at = (self.view.cursor as usize).min(names.len().saturating_sub(1));
        let position = |name: &str| listing.entries().iter().position(|e| e.name == name);
        let after = names.iter().skip(at).find_map(|name| position(name));
        let before = names.iter().take(at).rev().find_map(|name| position(name));
        let cursor = after.or(before).unwrap_or(0);
        self.keep_by_name(listing);
        self.view.cursor = cursor as u64;
        self.view.scroll = self.clamp(self.view.scroll.min(self.view.cursor));
        true
    }

    fn keep_by_name(&mut self, listing: Listing) {
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
        Arc::make_mut(&mut self.selection).mark_rows(keep);
    }

    pub(crate) fn set_listing(&mut self, listing: Listing) {
        self.selection = Arc::new(Selection::new(listing.len()));
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

    fn leave(&mut self, target: Location) -> (Location, Vec<String>) {
        let marks = self.marked_names();
        self.views.insert(self.location.clone(), self.view);
        if let Some(entry) = self.listing.as_ref().and_then(|l| l.get(self.view.cursor)) {
            self.names.insert(self.location.clone(), entry.name.clone());
        }
        self.listing = None;
        self.pending = None;
        self.selection = Arc::new(Selection::new(0));
        (std::mem::replace(&mut self.location, target), marks)
    }

    fn arrive(&mut self) {
        self.view = self.views.get(&self.location).copied().unwrap_or_default();
        self.restore = self.names.get(&self.location).cloned();
        let history = &self.history;
        self.views.retain(|location, _| history.contains(location));
        self.names.retain(|location, _| history.contains(location));
    }

    fn clamp(&self, row: u64) -> u64 {
        self.listing
            .as_ref()
            .map_or(row, |listing| row.min(listing.len().saturating_sub(1)))
    }
}
