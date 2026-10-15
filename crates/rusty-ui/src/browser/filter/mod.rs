use std::rc::Rc;

use rusty_core::{Event, Listing, Location, Pattern, TabId};
use rusty_fs::FsEntry;

use super::Browser;

pub(super) struct Filter {
    location: Location,
    text: String,
}

pub(super) struct Full {
    pub(super) listing: Listing,
    pub(super) details: Rc<[FsEntry]>,
}

struct Shown {
    listing: Listing,
    details: Rc<[FsEntry]>,
}

impl Browser {
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
        self.set_filter(tab, text)
    }

    pub fn filter_pop(&mut self) -> bool {
        let tab = self.state.active_tab().id();
        let Some(mut text) = self.filter_text(tab).map(str::to_owned) else {
            return false;
        };
        text.pop();
        self.set_filter(tab, text)
    }

    pub fn filter_clear(&mut self) -> bool {
        let tab = self.state.active_tab().id();
        self.filter_text(tab).is_some() && self.set_filter(tab, String::new())
    }

    pub(super) fn narrow(
        &mut self,
        tab: TabId,
        location: &Location,
        full: Full,
    ) -> (Listing, Rc<[FsEntry]>) {
        let shown = match self.filters.get(&tab) {
            Some(filter) if &filter.location == location => shown(&full, &filter.text),
            _ => None,
        };
        if shown.is_none() {
            self.filters.remove(&tab);
        }
        let shown = shown.unwrap_or_else(|| Shown {
            listing: full.listing.clone(),
            details: full.details.clone(),
        });
        self.full.insert(tab, full);
        (shown.listing, shown.details)
    }

    pub(super) fn drop_stale_filters(&mut self) {
        let state = &self.state;
        self.filters.retain(|tab, filter| {
            state
                .tab(*tab)
                .is_some_and(|tab| tab.location() == &filter.location)
        });
    }

    pub(super) fn remember_size(&mut self, tab: TabId, name: &str, bytes: u64) {
        let Some(full) = self.full.get_mut(&tab) else {
            return;
        };
        let mut rows = full.details.to_vec();
        if let Some(entry) = rows.iter_mut().find(|e| e.name.to_text() == name) {
            entry.size = bytes;
            full.details = rows.into();
        }
    }

    fn set_filter(&mut self, tab: TabId, text: String) -> bool {
        let Some(t) = self.state.tab(tab) else {
            return false;
        };
        if t.pending_request().is_some() {
            return false;
        }
        let location = t.location().clone();
        let Some(full) = self.full.get(&tab) else {
            return false;
        };
        let Some(shown) = shown(full, &text) else {
            return false;
        };
        if text.is_empty() {
            self.filters.remove(&tab);
        } else {
            let location = location.clone();
            self.filters.insert(tab, Filter { location, text });
        }
        self.details.insert(tab, shown.details);
        self.event(Event::ListingFiltered {
            tab,
            location,
            listing: shown.listing,
        });
        true
    }
}

fn shown(full: &Full, text: &str) -> Option<Shown> {
    if text.is_empty() {
        return Some(Shown {
            listing: full.listing.clone(),
            details: full.details.clone(),
        });
    }
    let pattern = Pattern::parse(&format!("{text}*")).ok()?;
    let keep: Vec<usize> = full
        .listing
        .entries()
        .iter()
        .enumerate()
        .filter(|(_, entry)| pattern.matches(&entry.name))
        .map(|(index, _)| index)
        .collect();
    if keep.is_empty() {
        return None;
    }
    let entries = keep
        .iter()
        .filter_map(|&index| full.listing.entries().get(index).cloned())
        .collect();
    let details: Vec<FsEntry> = keep
        .iter()
        .filter_map(|&index| full.details.get(index).cloned())
        .collect();
    Some(Shown {
        listing: Listing::new(entries),
        details: details.into(),
    })
}

#[cfg(test)]
mod tests;
