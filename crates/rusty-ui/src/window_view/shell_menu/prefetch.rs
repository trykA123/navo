use std::cell::RefCell;
use std::hash::{DefaultHasher, Hash as _, Hasher as _};
use std::time::Duration;

use gpui_kit::Task;
use rusty_core::{Location, PaneId, Selection, Tab};
use rusty_fs::{DriveClass, drive_class};

use super::FerriteWindow;
use super::provider::{MenuTarget, Reply, ShellMenuSession};
use super::targets::{self, Targets};
use crate::prelude::*;

const MENU_REST: Duration = Duration::from_millis(300);
const PRELOAD_KEPT: Duration = Duration::from_secs(10);
const PRELOAD_MAX_MARKED: u64 = 100;

type SessionReply = Reply<Box<dyn ShellMenuSession>>;

struct Prefetch {
    pane: PaneId,
    target: MenuTarget,
    extended: bool,
    reply: SessionReply,
    is_preload: bool,
    replaced: u64,
}

impl Prefetch {
    fn is_for(&self, pane: PaneId, target: &MenuTarget, extended: bool) -> bool {
        self.pane == pane && (&self.target, self.extended) == (target, extended)
    }
}

#[derive(Clone, PartialEq, Eq)]
struct MenuKey {
    pane: PaneId,
    location: Location,
    cursor: u64,
    marked: u64,
    marks: u64,
}

impl MenuKey {
    fn of(pane: PaneId, tab: &Tab) -> Self {
        Self {
            pane,
            location: tab.location().clone(),
            cursor: tab.view().cursor,
            marked: tab.selection().count(),
            marks: marks_hash(tab.selection()),
        }
    }

    fn is_at(&self, pane: PaneId, tab: &Tab) -> bool {
        self.pane == pane
            && self.cursor == tab.view().cursor
            && self.location == *tab.location()
            && self.marks == marks_hash(tab.selection())
    }
}

fn marks_hash(selection: &Selection) -> u64 {
    let mut hasher = DefaultHasher::new();
    selection.hash(&mut hasher);
    hasher.finish()
}

/// Network folders are read only when asked: their menus can stall the shell thread.
fn is_near(folder: &Location) -> bool {
    drive_class(folder) != DriveClass::Network
}

/// `None` when the read already failed; a read that answered keeps its answer for the menu.
fn unless_failed(reply: SessionReply) -> Option<SessionReply> {
    match reply() {
        Some(Err(_)) => None,
        None => Some(reply),
        answer => {
            let answer = RefCell::new(answer);
            Some(Box::new(move || answer.borrow_mut().take().or_else(&reply)))
        }
    }
}

/// At most one menu read ahead: for a right press, or where the cursor rests for `MENU_REST`, kept `PRELOAD_KEPT`.
#[derive(Default)]
pub(in crate::window_view) struct Prefetches {
    kept: Option<Prefetch>,
    key: Option<MenuKey>,
    has_seen: bool,
    wait: Option<Task<()>>,
}

impl FerriteWindow {
    pub(in crate::window_view) fn prefetch_shell_menu(
        &mut self,
        pane: PaneId,
        adds_cursor: bool,
        extended: bool,
    ) {
        let found = self.targets_with(pane, adds_cursor);
        let (Some(provider), Some(found)) = (self.shell_menus.clone(), found) else {
            self.prefetch.kept = None;
            return;
        };
        let target = MenuTarget::Items(found.items);
        let kept = self.prefetch.kept.as_mut();
        if let Some(kept) = kept.filter(|kept| kept.is_for(pane, &target, extended)) {
            kept.is_preload = false;
            return;
        }
        self.prefetch.kept = None;
        let replaced = provider.health().replaced;
        let reply = provider.open(&target, extended);
        self.prefetch.kept = Some(Prefetch {
            pane,
            target,
            extended,
            reply,
            is_preload: false,
            replaced,
        });
    }

    pub(in crate::window_view) fn drop_prefetch(&mut self) {
        self.prefetch.kept = None;
    }

    pub(super) fn take_prefetch(
        &mut self,
        pane: PaneId,
        target: &MenuTarget,
        extended: bool,
    ) -> Option<SessionReply> {
        let prefetch = self.prefetch.kept.take()?;
        let replaced = self.shell_menus.as_ref()?.health().replaced;
        let is_usable = prefetch.is_for(pane, target, extended) && prefetch.replaced == replaced;
        is_usable.then_some(prefetch.reply).and_then(unless_failed)
    }

    pub(super) fn menu_targets(&self, pane: PaneId) -> Option<Targets> {
        self.targets_with(pane, false)
    }

    fn targets_with(&self, pane: PaneId, adds_cursor: bool) -> Option<Targets> {
        let tab = self.browser.state().pane(pane)?.active();
        let found = targets::targets(tab, &self.browser.details(tab.id()), adds_cursor);
        (!found.items.is_empty()).then_some(found)
    }

    /// Restarts the rest timer when the cursor, the marks or the folder change; the first listed place does not count.
    pub(in crate::window_view) fn watch_menu_rest(&mut self, cx: &mut Context<Self>) {
        let place = self.menu_place();
        let is_same = match (&self.prefetch.key, place) {
            (Some(key), Some((pane, tab))) => key.is_at(pane, tab),
            (key, place) => key.is_none() && place.is_none(),
        };
        if is_same {
            return;
        }
        let key = place.map(|(pane, tab)| MenuKey::of(pane, tab));
        let rest = &mut self.prefetch;
        let is_first = !rest.has_seen && key.is_some();
        rest.has_seen |= is_first;
        rest.key.clone_from(&key);
        rest.wait = key.filter(|_| !is_first).map(|key| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(MENU_REST).await;
                let _ = this.update(cx, |view, _| view.preload_menu(&key));
                cx.background_executor().timer(PRELOAD_KEPT).await;
                let _ = this.update(cx, |view, _| view.drop_preload());
            })
        });
        self.drop_preload();
    }

    fn menu_place(&self) -> Option<(PaneId, &Tab)> {
        self.shell_menus.as_ref()?;
        let state = self.browser.state();
        let pane = state.focused_pane();
        let tab = state.pane(pane)?.active();
        tab.listing().map(|_| (pane, tab))
    }

    fn preload_menu(&mut self, key: &MenuKey) {
        let is_busy = self.menu_run.is_some() || self.prefetch.kept.is_some();
        let Some(provider) = self.shell_menus.clone().filter(|_| !is_busy) else {
            return;
        };
        let health = provider.health();
        let is_wanted = health.can_read_ahead
            && key.marked <= PRELOAD_MAX_MARKED
            && self.prefetch.key.as_ref() == Some(key)
            && is_near(&key.location);
        if !is_wanted {
            return;
        }
        let target = match self.menu_targets(key.pane) {
            Some(found) => MenuTarget::Items(found.items),
            None => MenuTarget::Background(key.location.clone()),
        };
        let reply = provider.open(&target, false);
        self.prefetch.kept = Some(Prefetch {
            pane: key.pane,
            target,
            extended: false,
            reply,
            is_preload: true,
            replaced: health.replaced,
        });
    }

    fn drop_preload(&mut self) {
        if self
            .prefetch
            .kept
            .as_ref()
            .is_some_and(|kept| kept.is_preload)
        {
            self.prefetch.kept = None;
        }
    }
}
