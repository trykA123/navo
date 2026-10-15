#![cfg_attr(
    not(test),
    expect(dead_code, reason = "the mutators get their caller, apply(), in C10")
)]

mod history;
mod listing;
mod pane;
mod tab;
mod window;

use std::collections::BTreeMap;

use crate::{Direction, LayoutError, Location, PaneId, TabId, WindowId};
pub use history::History;
pub use listing::{Entry, Listing};
pub use pane::Pane;
pub use tab::{Tab, ViewState};
pub use window::Window;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppState {
    focused: Window,
    others: BTreeMap<WindowId, Window>,
    next_id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StateError {
    #[error("window {} does not exist", .0.get())]
    UnknownWindow(WindowId),
    #[error("pane {} does not exist", .0.get())]
    UnknownPane(PaneId),
    #[error("the last window cannot be closed")]
    LastWindow,
    #[error("the last tab cannot be closed")]
    LastTab,
    #[error("there is no tab at index {0}")]
    NoSuchTab(usize),
    #[error("there is nothing to go back to")]
    NothingBack,
    #[error("there is nothing to go forward to")]
    NothingForward,
    #[error(transparent)]
    Layout(#[from] LayoutError),
}

impl AppState {
    #[must_use]
    pub fn new(home: Location) -> Self {
        let window = fresh_window(WindowId::new(1), PaneId::new(2), TabId::new(3), home);
        Self {
            focused: window,
            others: BTreeMap::new(),
            next_id: 4,
        }
    }

    #[must_use]
    pub fn window_ids(&self) -> Vec<WindowId> {
        let mut ids: Vec<WindowId> = self.others.keys().copied().collect();
        ids.push(self.focused.id());
        ids.sort_unstable();
        ids
    }

    #[must_use]
    pub fn focused_window(&self) -> WindowId {
        self.focused.id()
    }

    #[must_use]
    pub fn window(&self, id: WindowId) -> Option<&Window> {
        if self.focused.id() == id {
            Some(&self.focused)
        } else {
            self.others.get(&id)
        }
    }

    #[must_use]
    pub fn focused_pane(&self) -> PaneId {
        self.focused.focused_pane().id()
    }

    #[must_use]
    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        self.windows().find_map(|w| w.pane(id))
    }

    #[must_use]
    pub fn active_tab(&self) -> &Tab {
        self.focused.focused_pane().active()
    }

    pub(crate) fn active_tab_mut(&mut self) -> &mut Tab {
        self.focused.focused_pane_mut().active_mut()
    }

    pub(crate) fn focused_pane_mut(&mut self) -> &mut Pane {
        self.focused.focused_pane_mut()
    }

    pub(crate) fn new_tab(&mut self, location: Location) -> Tab {
        Tab::new(TabId::new(self.take_ids(1)), location)
    }

    pub(crate) fn split(&mut self, direction: Direction) -> Result<PaneId, StateError> {
        let pane_id = PaneId::new(self.next_id);
        let tab = Tab::new(
            TabId::new(self.next_id + 1),
            self.active_tab().location().clone(),
        );
        self.focused.split(direction, Pane::new(pane_id, tab))?;
        self.take_ids(2);
        Ok(pane_id)
    }

    pub(crate) fn close_pane(&mut self, pane: PaneId) -> Result<(), StateError> {
        if self.focused.contains(pane) {
            return self.focused.close(pane);
        }
        self.others
            .values_mut()
            .find(|w| w.contains(pane))
            .ok_or(StateError::UnknownPane(pane))?
            .close(pane)
    }

    pub(crate) fn focus_direction(&mut self, direction: Direction) -> Result<PaneId, StateError> {
        self.focused.focus_direction(direction)
    }

    pub(crate) fn open_window(&mut self, location: Location) -> WindowId {
        let first = self.take_ids(3);
        let id = WindowId::new(first);
        let window = fresh_window(id, PaneId::new(first + 1), TabId::new(first + 2), location);
        let old = std::mem::replace(&mut self.focused, window);
        self.others.insert(old.id(), old);
        id
    }

    pub(crate) fn close_window(&mut self, id: WindowId) -> Result<(), StateError> {
        if self.window(id).is_none() {
            return Err(StateError::UnknownWindow(id));
        }
        if id != self.focused.id() {
            self.others.remove(&id);
            return Ok(());
        }
        self.focused = self.others.pop_first().ok_or(StateError::LastWindow)?.1;
        Ok(())
    }

    pub(crate) fn focus_window(&mut self, id: WindowId) -> Result<(), StateError> {
        if id == self.focused.id() {
            return Ok(());
        }
        let window = self
            .others
            .remove(&id)
            .ok_or(StateError::UnknownWindow(id))?;
        let old = std::mem::replace(&mut self.focused, window);
        self.others.insert(old.id(), old);
        Ok(())
    }

    fn windows(&self) -> impl Iterator<Item = &Window> {
        std::iter::once(&self.focused).chain(self.others.values())
    }

    fn take_ids(&mut self, count: u64) -> u64 {
        let first = self.next_id;
        self.next_id += count;
        first
    }
}

fn fresh_window(id: WindowId, pane: PaneId, tab: TabId, location: Location) -> Window {
    Window::new(id, Pane::new(pane, Tab::new(tab, location)))
}

#[cfg(test)]
mod tests;
