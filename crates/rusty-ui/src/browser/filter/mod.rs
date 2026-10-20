use std::rc::Rc;

use rusty_core::{Event, Listing, Location, Pattern, TabId};
use rusty_fs::FsEntry;

use super::Browser;

mod marks;
pub(super) use marks::HiddenMarks;

pub(super) struct Filter {
    location: Location,
    text: String,
}

#[derive(Clone)]
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
    pub fn shows_hidden(&self) -> bool {
        self.show_hidden
    }

    #[must_use]
    pub fn shows_system(&self) -> bool {
        self.show_system
    }

    pub fn set_show_hidden(&mut self, show: bool) {
        if self.show_hidden != show {
            self.show_hidden = show;
            self.reshow_all();
        }
    }

    pub fn set_show_system(&mut self, show: bool) {
        if self.show_system != show {
            self.show_system = show;
            self.reshow_all();
        }
    }

    fn visibility(&self) -> Visibility {
        Visibility {
            hidden: self.show_hidden,
            system: self.show_system,
        }
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
            Some(filter) if &filter.location == location => {
                shown(&full, &filter.text, self.visibility())
            }
            _ => None,
        };
        if shown.is_none() {
            self.filters.remove(&tab);
        }
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
            Pattern::parse(&format!("{}*", filter.text)).is_ok_and(|p| !p.matches(name))
        });
        if hidden {
            self.filters.remove(&tab);
        }
    }

    pub(super) fn drop_stale_filters(&mut self) {
        self.drop_hidden_marks();
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

    pub(super) fn set_filter(&mut self, tab: TabId, text: String) -> bool {
        let Some(t) = self.state.tab(tab) else {
            return false;
        };
        if t.listing().is_none() {
            return false;
        }
        let location = t.location().clone();
        let Some(full) = self.full.get(&tab) else {
            return false;
        };
        let Some(shown) = shown(full, &text, self.visibility()) else {
            return false;
        };
        if text.is_empty() {
            self.filters.remove(&tab);
        } else {
            let location = location.clone();
            self.filters.insert(tab, Filter { location, text });
        }
        self.details.insert(tab, shown.details);
        self.hide_marks(tab, &shown.listing);
        self.event(Event::ListingFiltered {
            tab,
            location,
            listing: shown.listing,
        });
        self.remark_hidden(tab);
        true
    }
}

#[derive(Clone, Copy)]
struct Visibility {
    hidden: bool,
    system: bool,
}

fn is_visible(entry: Option<&FsEntry>, shows: Visibility) -> bool {
    entry.is_none_or(|e| {
        let is_hidden = e.attributes.hidden || e.attributes.system;
        let is_system = e.attributes.system;
        (shows.hidden || !is_hidden) && (shows.system || !is_system)
    })
}

fn shown(full: &Full, text: &str, shows: Visibility) -> Option<Shown> {
    if text.is_empty() && shows.hidden && shows.system {
        return Some(Shown {
            listing: full.listing.clone(),
            details: full.details.clone(),
        });
    }
    let pattern = if text.is_empty() {
        None
    } else {
        Some(Pattern::parse(&format!("{text}*")).ok()?)
    };
    let keep: Vec<usize> = full
        .listing
        .entries()
        .iter()
        .enumerate()
        .filter(|(index, _)| is_visible(full.details.get(*index), shows))
        .filter(|(_, entry)| pattern.as_ref().is_none_or(|p| p.matches(&entry.name)))
        .map(|(index, _)| index)
        .collect();
    if keep.is_empty() && pattern.is_some() {
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
mod hidden_tests;
#[cfg(test)]
mod marks_tests;
#[cfg(test)]
mod tests;
