use std::collections::HashMap;

use navo_core::{Command, CursorMove, Entry, Event, Listing, Location, TabId};
use navo_fs::CancelToken;
use navo_fs::view::{Rows, ViewFilter, filter_all, filter_order};

use super::Browser;
use super::sort::listing_of;
use super::view_jobs::ViewChange;
use crate::name_match::{Needle, Scratch};

mod marks;
pub(super) use marks::HiddenMarks;

pub(super) struct Filter {
    location: Location,
    text: String,
}

#[derive(Clone)]
pub(super) struct Full {
    pub(super) listing: Listing,
    pub(super) details: Rows,
}

pub(super) struct Shown {
    listing: Listing,
    details: Rows,
}

impl Browser {
    #[must_use]
    pub fn shows_hidden(&self) -> bool {
        self.shows.hidden
    }

    #[must_use]
    pub fn shows_system(&self) -> bool {
        self.shows.system
    }

    pub fn set_show_hidden(&mut self, show: bool) {
        if self.shows.hidden != show {
            self.shows.hidden = show;
            self.reshow_all();
        }
    }

    pub fn set_show_system(&mut self, show: bool) {
        if self.shows.system != show {
            self.shows.system = show;
            self.reshow_all();
        }
    }

    pub(super) fn visibility(&self) -> Visibility {
        self.shows
    }

    fn reshow_all(&mut self) {
        let tabs: Vec<TabId> = self.full.keys().copied().collect();
        for tab in tabs {
            self.reshow(tab);
        }
    }

    fn reshow(&mut self, tab: TabId) {
        let Some(location) = self.state.tab(tab).map(|t| t.location().clone()) else {
            return;
        };
        let shows = self.visibility();
        let Some(full) = self.full.get(&tab) else {
            return;
        };
        let text = self.filter_text(tab).unwrap_or_default();
        let shown = shown(full, text, shows).or_else(|| {
            self.filters.remove(&tab);
            shown(full, "", shows)
        });
        let Some(shown) = shown else {
            return;
        };
        self.details.insert(tab, shown.details);
        self.hide_marks(tab, &shown.listing);
        self.event(Event::ListingRefiltered {
            tab,
            location,
            listing: shown.listing,
        });
        self.remark_hidden(tab);
    }

    #[must_use]
    pub fn filter_text(&self, tab: TabId) -> Option<&str> {
        let location = self.state.tab(tab)?.location();
        self.filters
            .get(&tab)
            .filter(|filter| &filter.location == location)
            .map(|filter| filter.text.as_str())
    }

    pub fn filter_push(&mut self, ch: char) -> bool {
        if ch.is_whitespace() || ch.is_control() {
            return false;
        }
        let tab = self.state.active_tab().id();
        let mut text = self.filter_text(tab).unwrap_or_default().to_owned();
        text.push(ch);
        self.set_filter(tab, text) && self.land_unless_pending(tab)
    }

    pub fn filter_pop(&mut self) -> bool {
        let tab = self.state.active_tab().id();
        let Some(mut text) = self.filter_text(tab).map(str::to_owned) else {
            return false;
        };
        text.pop();
        self.set_filter(tab, text) && self.land_unless_pending(tab)
    }

    pub fn filter_clear(&mut self) -> bool {
        let tab = self.state.active_tab().id();
        self.filter_text(tab).is_some() && self.set_filter(tab, String::new())
    }

    fn land_unless_pending(&mut self, tab: TabId) -> bool {
        self.views_off_thread(tab) || self.land_cursor(tab)
    }

    pub(super) fn land_cursor(&mut self, tab: TabId) -> bool {
        let Some(text) = self.filter_text(tab).filter(|text| !text.is_empty()) else {
            return true;
        };
        let needle = Needle::parse(text);
        let mut scratch = Scratch::default();
        let row = self
            .state
            .tab(tab)
            .and_then(|t| t.listing())
            .and_then(|listing| {
                listing
                    .entries()
                    .iter()
                    .position(|entry| needle.starts(&entry.name, &mut scratch))
            })
            .unwrap_or(0);
        let to = CursorMove::Row(u64::try_from(row).unwrap_or(0));
        self.command(&Command::PaneCursorMove { to }).is_ok()
    }

    pub(super) fn narrow(
        &mut self,
        tab: TabId,
        location: &Location,
        full: Full,
    ) -> (Listing, Rows) {
        let shown = match self.filters.get(&tab) {
            Some(filter) if &filter.location == location => {
                shown(&full, &filter.text, self.visibility())
            }
            _ => None,
        };
        if shown.is_none() {
            self.filters.remove(&tab);
        }
        self.view_jobs.cancel(tab);
        let shown = shown
            .or_else(|| self::shown(&full, "", self.visibility()))
            .unwrap_or_else(|| Shown {
                listing: full.listing.clone(),
                details: full.details.clone(),
            });
        self.prune_hidden(tab, &full.listing);
        self.full.insert(tab, full);
        (shown.listing, shown.details)
    }

    pub(super) fn reveal_name(&mut self, tab: TabId, name: &str) {
        let hidden = self.filters.get(&tab).is_some_and(|filter| {
            !Needle::parse(&filter.text).matches(name, &mut Scratch::default())
        });
        if hidden {
            self.filters.remove(&tab);
        }
        self.view_jobs.cancel(tab);
    }

    pub(super) fn drop_stale_filters(&mut self) {
        self.drop_hidden_marks();
        self.view_jobs.drop_stale(&self.state);
        let state = &self.state;
        self.filters.retain(|tab, filter| {
            state
                .tab(*tab)
                .is_some_and(|tab| tab.location() == &filter.location)
        });
        let is_open = |tab: &TabId| state.tab(*tab).is_some();
        self.cached.retain(|tab, _| is_open(tab));
        self.stalls.retain(is_open);
        self.full.retain(|tab, _| is_open(tab));
        self.details.retain(|tab, _| is_open(tab));
        self.sorted.retain(|tab, _| is_open(tab));
    }

    pub(super) fn remember_sizes(&mut self, tab: TabId, sizes: &[(String, u64)]) {
        let Some(full) = self.full.get_mut(&tab) else {
            return;
        };
        let wanted: HashMap<&str, u64> = sizes
            .iter()
            .map(|(name, bytes)| (name.as_str(), *bytes))
            .collect();
        let mut rows = full.details.to_vec();
        super::size_rows::count_copy();
        let mut found = Vec::new();
        for entry in &mut rows {
            let name = entry.name.to_text();
            if let Some(&bytes) = wanted.get(name.as_str()) {
                entry.size = bytes;
                found.push((name, bytes));
            }
        }
        if found.is_empty() {
            return;
        }
        let rows: Rows = rows.into();
        self.view_jobs.resized(tab, (&full.details, &rows), &found);
        full.details = rows;
    }

    pub(super) fn set_filter(&mut self, tab: TabId, text: String) -> bool {
        let Some(t) = self.state.tab(tab) else {
            return false;
        };
        if t.listing().is_none() {
            return false;
        }
        let location = t.location().clone();
        if self.views_off_thread(tab) {
            return self.request_view(tab, ViewChange::Text(text));
        }
        let Some(full) = self.full.get(&tab) else {
            return false;
        };
        let Some(shown) = shown(full, &text, self.visibility()) else {
            return false;
        };
        self.store_filter(tab, &location, text);
        self.show_filtered(tab, location, shown);
        true
    }

    pub(super) fn store_filter(&mut self, tab: TabId, location: &Location, text: String) {
        if text.is_empty() {
            self.filters.remove(&tab);
        } else {
            let location = location.clone();
            self.filters.insert(tab, Filter { location, text });
        }
    }

    pub(super) fn show_filtered(&mut self, tab: TabId, location: Location, shown: Shown) {
        self.details.insert(tab, shown.details);
        self.hide_marks(tab, &shown.listing);
        self.event(Event::ListingFiltered {
            tab,
            location,
            listing: shown.listing,
        });
        self.remark_hidden(tab);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Visibility {
    hidden: bool,
    system: bool,
}

impl Visibility {
    pub(super) const ALL: Self = Self {
        hidden: true,
        system: true,
    };
}

fn is_reserved(entry: &Entry) -> bool {
    navo_core::brand::is_reserved_name(&entry.name)
}

fn shown(full: &Full, text: &str, shows: Visibility) -> Option<Shown> {
    shown_until(full, text, shows, &CancelToken::new())
}

pub(super) fn shown_until(
    full: &Full,
    text: &str,
    shows: Visibility,
    cancel: &CancelToken,
) -> Option<Shown> {
    let has_reserved = text.is_empty() && full.listing.entries().iter().any(is_reserved);
    if text.is_empty() && shows.hidden && shows.system && !has_reserved {
        return Some(Shown {
            listing: full.listing.clone(),
            details: full.details.clone(),
        });
    }
    let needle = (!text.is_empty()).then(|| Needle::parse(text));
    let filter = ViewFilter {
        needle: needle.as_ref(),
        show_hidden: shows.hidden,
        show_system: shows.system,
    };
    let source = full.details.source();
    let keep = match full.details.order() {
        Some(order) => filter_order(source, order, &filter, cancel)?,
        None => filter_all(source, &filter, cancel)?,
    };
    if keep.is_empty() && needle.is_some() {
        return None;
    }
    let details = full.details.narrowed(keep);
    Some(Shown {
        listing: listing_of(&details),
        details,
    })
}

#[cfg(test)]
mod hidden_tests;
#[cfg(test)]
mod marks_tests;
#[cfg(test)]
mod tests;
