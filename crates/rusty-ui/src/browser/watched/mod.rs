mod recheck;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use rusty_core::{Location, ProviderPath, TabId, relist_tab};
use rusty_fs::{Watch, WatchChange, WatchEvent, WatchSink};

use super::{Browser, Message};
use recheck::Recheck;

const PATCH_LIMIT: usize = 64;
const NETWORK_RECHECK: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq)]
enum Refresh {
    Patch(Vec<WatchChange>),
    Full,
}

fn is_short_name(name: &str) -> bool {
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    let Some((head, digits)) = stem.split_once('~') else {
        return false;
    };
    stem.len() <= 8
        && ext.len() <= 3
        && !head.is_empty()
        && !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
}

impl Refresh {
    fn of(changes: Vec<WatchChange>) -> Self {
        let names: HashSet<&ProviderPath> = changes.iter().flat_map(names_of).collect();
        let has_short_name = names.iter().any(|name| is_short_name(&name.to_text()));
        if names.len() > PATCH_LIMIT || has_short_name {
            Self::Full
        } else {
            Self::Patch(changes)
        }
    }

    fn merge(self, next: Self) -> Self {
        match (self, next) {
            (Self::Patch(mut first), Self::Patch(then)) => {
                let fresh: Vec<WatchChange> = then
                    .into_iter()
                    .filter(|change| !first.contains(change))
                    .collect();
                first.extend(fresh);
                Self::of(first)
            }
            _ => Self::Full,
        }
    }
}

fn names_of(change: &WatchChange) -> Vec<&ProviderPath> {
    match change {
        WatchChange::Created(name) | WatchChange::Removed(name) | WatchChange::Modified(name) => {
            vec![name]
        }
        WatchChange::Renamed { from, to } => vec![from, to],
    }
}

enum Folder {
    Live { watch: Watch },
    Fallback { _recheck: Option<Recheck> },
}

#[derive(Default)]
pub(super) struct Watching {
    folders: HashMap<Location, Folder>,
    queued: HashMap<TabId, (Location, Refresh)>,
    patches: HashMap<TabId, Vec<WatchChange>>,
}

impl Browser {
    #[must_use]
    pub fn is_watched(&self, location: &Location) -> bool {
        matches!(
            self.watching.folders.get(location),
            Some(Folder::Live { watch }) if watch.is_live()
        )
    }

    pub(super) fn is_settled(&self, location: &Location) -> bool {
        self.is_watched(location)
            && !self
                .watching
                .queued
                .values()
                .any(|(queued, _)| queued == location)
    }

    pub fn revalidate_unwatched(&mut self) -> bool {
        let fallbacks: Vec<Location> = self
            .watching
            .folders
            .iter()
            .filter(|(_, folder)| matches!(folder, Folder::Fallback { .. }))
            .map(|(location, _)| location.clone())
            .collect();
        let mut relisted = false;
        for location in &fallbacks {
            relisted |= self.refresh_folder(location, &Refresh::Full);
        }
        relisted
    }

    pub(super) fn revalidate_focused(&mut self) {
        let location = self.state.active_tab().location().clone();
        if matches!(
            self.watching.folders.get(&location),
            Some(Folder::Fallback { .. })
        ) {
            self.refresh_folder(&location, &Refresh::Full);
        }
    }

    pub(super) fn release_watches(&mut self) {
        release(std::mem::take(&mut self.watching.folders).into_values());
    }

    pub(super) fn take_patch(&mut self, tab: TabId) -> Option<Vec<WatchChange>> {
        self.watching.patches.remove(&tab)
    }

    pub(super) fn sync_watches(&mut self) {
        if !self.provider.capabilities().watch {
            return;
        }
        let open = self.open_tabs();
        self.watching
            .queued
            .retain(|tab, (at, _)| open.iter().any(|(open, shown)| open == tab && shown == at));
        let wanted: HashSet<Location> = open.into_iter().map(|(_, location)| location).collect();
        let closed: Vec<Folder> = self
            .watching
            .folders
            .extract_if(|location, _| !wanted.contains(location))
            .map(|(_, folder)| folder)
            .collect();
        release(closed);
        let started: Vec<Location> = wanted
            .into_iter()
            .filter(|location| !self.watching.folders.contains_key(location))
            .collect();
        for location in &started {
            let folder = self.start_folder(location);
            self.watching.folders.insert(location.clone(), folder);
        }
        for location in &started {
            let idle = self.idle_tabs_at(location);
            self.refresh_tabs(idle, location, &Refresh::Full);
        }
    }

    pub(super) fn accept_watched(&mut self, location: &Location, event: WatchEvent) -> bool {
        if !self.watching.folders.contains_key(location) {
            return false;
        }
        self.forget_profiles(location, &event);
        let refresh = match event {
            WatchEvent::Changed(changes) => Refresh::of(changes),
            WatchEvent::Overflow => Refresh::Full,
            WatchEvent::Failed(_) => {
                let folder = self.fallback(location);
                release(self.watching.folders.insert(location.clone(), folder));
                Refresh::Full
            }
        };
        self.refresh_folder(location, &refresh)
    }

    fn forget_profiles(&self, location: &Location, event: &WatchEvent) {
        let WatchEvent::Changed(changes) = event else {
            self.profiles.invalidate(location);
            return;
        };
        if changes.len() > PATCH_LIMIT {
            self.profiles.invalidate(location);
            return;
        }
        for name in changes.iter().flat_map(names_of) {
            match location.join(name) {
                Ok(changed) => self.profiles.invalidate(&changed),
                Err(_) => self.profiles.invalidate(location),
            }
        }
    }

    pub(super) fn accept_recheck(&mut self, location: &Location) -> bool {
        let is_fallback = matches!(
            self.watching.folders.get(location),
            Some(Folder::Fallback { .. })
        );
        if !is_fallback {
            return false;
        }
        let idle = self.idle_tabs_at(location);
        self.refresh_tabs(idle, location, &Refresh::Full)
    }

    pub(super) fn drain_refresh(&mut self, tab: TabId) -> bool {
        if !self.is_idle(tab) {
            return false;
        }
        let Some((_, refresh)) = self.watching.queued.remove(&tab) else {
            return false;
        };
        if let Refresh::Patch(changes) = refresh {
            self.follow_renames(tab, &changes);
            self.watching.patches.insert(tab, changes);
        }
        let effects = relist_tab(&mut self.state, tab);
        self.run(effects);
        true
    }

    fn is_idle(&self, tab: TabId) -> bool {
        self.state
            .tab(tab)
            .is_some_and(|t| t.pending_request().is_none() && t.listing().is_some())
    }

    fn tabs_at(&self, location: &Location) -> Vec<TabId> {
        self.open_tabs()
            .into_iter()
            .filter(|(_, open)| open == location)
            .map(|(tab, _)| tab)
            .collect()
    }

    fn idle_tabs_at(&self, location: &Location) -> Vec<TabId> {
        let mut tabs = self.tabs_at(location);
        tabs.retain(|tab| self.is_idle(*tab));
        tabs
    }

    fn refresh_folder(&mut self, location: &Location, refresh: &Refresh) -> bool {
        let tabs = self.tabs_at(location);
        self.refresh_tabs(tabs, location, refresh)
    }

    fn refresh_tabs(&mut self, tabs: Vec<TabId>, location: &Location, refresh: &Refresh) -> bool {
        for tab in &tabs {
            let merged = match self.watching.queued.remove(tab) {
                Some((at, queued)) if at == *location => queued.merge(refresh.clone()),
                _ => refresh.clone(),
            };
            self.watching
                .queued
                .insert(*tab, (location.clone(), merged));
        }
        let mut relisted = false;
        for tab in tabs {
            relisted |= self.drain_refresh(tab);
        }
        relisted
    }

    fn open_tabs(&self) -> Vec<(TabId, Location)> {
        self.state
            .window_ids()
            .into_iter()
            .filter_map(|window| self.state.window(window))
            .flat_map(|window| window.panes())
            .map(|pane| (pane.active().id(), pane.active().location().clone()))
            .collect()
    }

    fn start_folder(&self, location: &Location) -> Folder {
        let (sender, wake, at) = (
            self.sender.clone(),
            Arc::clone(&self.wake),
            location.clone(),
        );
        let sink: WatchSink = Box::new(move |event| {
            let _ = sender.send(Message::Watched(at.clone(), event));
            wake();
        });
        match self.provider.watch(location, sink) {
            Ok(watch) => Folder::Live { watch },
            Err(_) => self.fallback(location),
        }
    }

    fn fallback(&self, location: &Location) -> Folder {
        if !self.keys.is_network(location) {
            return Folder::Fallback { _recheck: None };
        }
        let (sender, wake, at) = (
            self.sender.clone(),
            Arc::clone(&self.wake),
            location.clone(),
        );
        let recheck = Recheck::start(NETWORK_RECHECK, move || {
            let is_sent = sender.send(Message::Recheck(at.clone())).is_ok();
            wake();
            is_sent
        });
        Folder::Fallback { _recheck: recheck }
    }

    fn follow_renames(&mut self, tab: TabId, changes: &[WatchChange]) {
        let Some(mut cursor) = self.state.tab(tab).and_then(|t| {
            t.listing()
                .and_then(|l| l.get(t.view().cursor))
                .map(|e| e.name.clone())
        }) else {
            return;
        };
        let mut is_moved = false;
        for change in changes {
            if let WatchChange::Renamed { from, to } = change
                && from.to_text() == cursor
            {
                cursor = to.to_text();
                is_moved = true;
            }
        }
        if is_moved {
            self.focus_after_load.insert(tab, cursor);
        }
    }
}

fn release(folders: impl IntoIterator<Item = Folder>) {
    let folders: Vec<Folder> = folders.into_iter().collect();
    if folders.is_empty() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("watch-release".to_owned())
        .spawn(move || drop(folders));
}

#[cfg(test)]
mod tests;
