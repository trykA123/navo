use rusty_core::{Location, PaneId};

use super::FerriteWindow;
use super::provider::{Reply, ShellMenuSession};
use super::targets::{self, Targets};

pub(in crate::window_view) struct Prefetch {
    pane: PaneId,
    items: Vec<Location>,
    reply: Reply<Box<dyn ShellMenuSession>>,
}

impl FerriteWindow {
    pub(in crate::window_view) fn prefetch_shell_menu(&mut self, pane: PaneId, adds_cursor: bool) {
        self.prefetch = None;
        let Some(provider) = self.shell_menus.clone() else {
            return;
        };
        if let Some(found) = self.targets_with(pane, adds_cursor) {
            let reply = provider.open(&found.items);
            self.prefetch = Some(Prefetch {
                pane,
                items: found.items,
                reply,
            });
        }
    }

    pub(in crate::window_view) fn drop_prefetch(&mut self) {
        self.prefetch = None;
    }

    pub(super) fn take_prefetch(
        &mut self,
        pane: PaneId,
        items: &[Location],
    ) -> Option<Reply<Box<dyn ShellMenuSession>>> {
        let prefetch = self.prefetch.take()?;
        (prefetch.pane == pane && prefetch.items == items).then_some(prefetch.reply)
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
