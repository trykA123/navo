use std::cell::RefCell;

use rusty_core::PaneId;

use super::FerriteWindow;
use super::provider::{MenuTarget, Reply, ShellMenuSession};
use super::targets::{self, Targets};

type SessionReply = Reply<Box<dyn ShellMenuSession>>;

struct Prefetch {
    pane: PaneId,
    target: MenuTarget,
    extended: bool,
    reply: SessionReply,
    replaced: u64,
}

impl Prefetch {
    fn is_for(&self, pane: PaneId, target: &MenuTarget, extended: bool) -> bool {
        self.pane == pane && (&self.target, self.extended) == (target, extended)
    }
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

/// At most one menu read ahead, started by a right press.
#[derive(Default)]
pub(in crate::window_view) struct Prefetches {
    kept: Option<Prefetch>,
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
        let kept = self.prefetch.kept.as_ref();
        if kept.is_some_and(|kept| kept.is_for(pane, &target, extended)) {
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
}
