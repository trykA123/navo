use std::collections::HashMap;
use std::sync::Arc;

use futures::StreamExt as _;
use futures::channel::mpsc::unbounded;
use navo_core::{Location, PaneId};
use navo_fs::{EntryKind, FsEntry};
use navo_l10n::t;

use super::NavoWindow;
use super::lane::Reply;
use crate::git::{FolderBadges, GitHooks};
use crate::prelude::*;

type Read = (PaneId, Location, Option<Arc<FolderBadges>>);

struct Seen {
    location: Location,
    rows: Arc<[FsEntry]>,
    generation: u64,
}

pub(super) struct GitBadgeStore {
    hooks: Option<Arc<dyn GitHooks>>,
    seen: HashMap<PaneId, Seen>,
    latest: HashMap<PaneId, Arc<FolderBadges>>,
    generation: u64,
}

fn folder_names(rows: &[FsEntry]) -> Vec<String> {
    rows.iter()
        .filter(|row| matches!(row.kind, EntryKind::Folder))
        .map(|row| row.name.to_text())
        .collect()
}

impl GitBadgeStore {
    pub(super) fn new(hooks: Option<Arc<dyn GitHooks>>, cx: &mut Context<NavoWindow>) -> Self {
        if let Some(hooks) = &hooks {
            Self::listen(hooks.as_ref(), cx);
        }
        Self {
            hooks,
            seen: HashMap::new(),
            latest: HashMap::new(),
            generation: 0,
        }
    }

    fn listen(hooks: &dyn GitHooks, cx: &mut Context<NavoWindow>) {
        let (sender, mut receiver) = unbounded::<()>();
        hooks.subscribe(Box::new(move || {
            let _ = sender.unbounded_send(());
        }));
        cx.spawn(async move |this, cx| {
            while receiver.next().await.is_some()
                && this.update(cx, NavoWindow::git_changed).is_ok()
            {}
        })
        .detach();
    }

    fn accept_all(&mut self, read: Vec<Read>) -> bool {
        read.into_iter().fold(false, |any, (pane, at, fresh)| {
            let changed = match fresh {
                Some(fresh) => self.accept(pane, &at, fresh),
                None => self.clear(pane, &at),
            };
            changed || any
        })
    }

    pub(super) fn hooks(&self) -> Option<Arc<dyn GitHooks>> {
        self.hooks.clone()
    }

    pub(super) fn badges(&self, pane: PaneId) -> Option<Arc<FolderBadges>> {
        self.latest.get(&pane).cloned()
    }

    fn unchanged(&self, pane: PaneId, location: &Location, rows: &Arc<[FsEntry]>) -> bool {
        self.seen
            .get(&pane)
            .is_some_and(|s| s.location.same_place(location) && Arc::ptr_eq(&s.rows, rows))
    }

    fn shares(&self, location: &Location) -> bool {
        self.seen.values().any(|s| s.location.same_place(location))
    }

    /// The hooks hold no badges for `location` any more: the scanner stopped or was replaced.
    fn clear(&mut self, pane: PaneId, location: &Location) -> bool {
        let shows = self
            .seen
            .get(&pane)
            .is_some_and(|seen| seen.location.same_place(location));
        shows && self.latest.remove(&pane).is_some()
    }

    fn accept(&mut self, pane: PaneId, location: &Location, fresh: Arc<FolderBadges>) -> bool {
        let Some(seen) = self.seen.get(&pane) else {
            return false;
        };
        if !seen.location.same_place(location) || fresh.revision < seen.generation {
            return false;
        }
        let newer = self
            .latest
            .get(&pane)
            .is_none_or(|old| !Arc::ptr_eq(old, &fresh) && fresh.revision >= old.revision);
        if newer {
            self.latest.insert(pane, fresh);
        }
        newer
    }
}

impl NavoWindow {
    pub(super) fn sync_git(&mut self) {
        let Some(hooks) = self.git_badges.hooks.clone() else {
            return;
        };
        let panes = self.panes();
        let gone: Vec<PaneId> = self
            .git_badges
            .seen
            .keys()
            .filter(|pane| !panes.contains(pane))
            .copied()
            .collect();
        for pane in gone {
            self.git_leave(pane, &hooks);
        }
        for pane in panes {
            self.git_sync_pane(pane, &hooks);
        }
    }

    fn git_sync_pane(&mut self, pane: PaneId, hooks: &Arc<dyn GitHooks>) {
        let Some(tab) = self.browser.state().pane(pane).map(|p| p.active()) else {
            return;
        };
        let location = tab.location().clone();
        let rows = self.browser.details(tab.id());
        if self.git_badges.unchanged(pane, &location, &rows) {
            return;
        }
        let names = if location.is_local() {
            folder_names(&rows)
        } else {
            Vec::new()
        };
        self.git_leave(pane, hooks);
        if names.is_empty() {
            return;
        }
        self.git_badges.generation += 1;
        let generation = self.git_badges.generation;
        let seen = Seen {
            location: location.clone(),
            rows,
            generation,
        };
        self.git_badges.seen.insert(pane, seen);
        let hooks = Arc::clone(hooks);
        self.lane.push(Box::new(move || {
            hooks.observe(&location, generation, names);
            Self::no_reply()
        }));
    }

    fn git_leave(&mut self, pane: PaneId, hooks: &Arc<dyn GitHooks>) {
        self.git_badges.latest.remove(&pane);
        let Some(old) = self.git_badges.seen.remove(&pane) else {
            return;
        };
        if self.git_badges.shares(&old.location) {
            return;
        }
        let hooks = Arc::clone(hooks);
        self.lane.push(Box::new(move || {
            hooks.forget(&old.location);
            Self::no_reply()
        }));
    }

    fn git_changed(&mut self, cx: &mut Context<Self>) {
        let Some(hooks) = self.git_badges.hooks.clone() else {
            return;
        };
        if self.settings.is_open() {
            cx.notify();
        }
        let watched: Vec<(PaneId, Location)> = self
            .git_badges
            .seen
            .iter()
            .map(|(pane, seen)| (*pane, seen.location.clone()))
            .collect();
        self.lane.push(Box::new(move || {
            let read: Vec<Read> = watched
                .into_iter()
                .map(|(pane, at)| {
                    let badges = hooks.badges(&at);
                    (pane, at, badges)
                })
                .collect();
            Box::new(move |view, window, cx| view.git_read(read, window, cx))
        }));
    }

    fn git_read(&mut self, read: Vec<Read>, window: &mut Window, cx: &mut Context<Self>) {
        if self.git_badges.accept_all(read) {
            self.queue_cached_refresh(window, cx);
        }
    }

    /// The quiet footer note for a pane whose listing the Git scanner skipped as too slow.
    pub(super) fn slow_drive_note(&self, pane: PaneId) -> Option<SharedString> {
        let badges = self.git_badges.badges(pane)?;
        badges.slow_drive.then(|| t!("git-slow-drive").into())
    }

    fn no_reply() -> Reply {
        Box::new(|_, _, _| {})
    }
}
