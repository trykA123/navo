mod history;
mod listing;
mod pane;
mod tab;
mod tab_move;
mod window;
mod workspace;

use std::collections::BTreeMap;

use crate::placement::{MonitorSetup, Placement};
use crate::{Applied, Direction, LayoutError, Location, PaneId, SplitPath, TabId, Tiles, WindowId};
pub use history::History;
pub use listing::{Entry, Listing};
pub use pane::Pane;
pub use tab::{Tab, ViewState};
pub use tab_move::TabMove;
pub use window::Window;
pub(crate) use workspace::workspace_name;
pub use workspace::{
    MAX_WORKSPACES, SAVE_ON_SWITCH, Snapshot, Workspace, WorkspaceAction, changes,
};

#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    focused: Window,
    others: BTreeMap<WindowId, Window>,
    next_id: u64,
    next_request: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StateError {
    #[error("window {} does not exist", .0.get())]
    UnknownWindow(WindowId),
    #[error("pane {} does not exist", .0.get())]
    UnknownPane(PaneId),
    #[error("there is no pane at number {0}")]
    NoSuchPane(usize),
    #[error("Four panes is the most in this version.")]
    TooManyPanes,
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
    #[error("There is no workspace {0}.")]
    NoWorkspace(usize),
    #[error("a window can have at most nine workspaces")]
    WorkspaceLimit,
    #[error("the last workspace cannot be deleted")]
    LastWorkspace,
    #[error("a workspace name must contain 1 to 40 characters")]
    InvalidWorkspaceName,
    #[error("a workspace can only move by -1 or 1")]
    InvalidWorkspaceMove,
    #[error("there is no deleted workspace to restore")]
    NothingToRestore,
    #[error("invalid workspace: {0}")]
    InvalidWorkspace(String),
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
            next_request: 1,
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

    pub fn remember_placement(&mut self, setup: &MonitorSetup, placement: Placement) {
        self.focused.remember(setup, placement);
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

    #[must_use]
    pub fn last_marks(&self) -> Option<&[String]> {
        self.focused.last_marks()
    }

    pub fn remember_marks(&mut self, names: Vec<String>) {
        self.focused.remember_marks(names);
    }

    pub(crate) fn focused_pane_mut(&mut self) -> &mut Pane {
        self.focused.focused_pane_mut()
    }

    pub(crate) fn restore(
        windows: Vec<Window>,
        focused: WindowId,
        next_id: u64,
    ) -> Result<Self, String> {
        let mut others: BTreeMap<WindowId, Window> =
            windows.into_iter().map(|w| (w.id(), w)).collect();
        let focused = others
            .remove(&focused)
            .ok_or_else(|| format!("focused window {} does not exist", focused.get()))?;
        Ok(Self {
            focused,
            others,
            next_id,
            next_request: 1,
        })
    }

    pub(crate) fn take_request(&mut self) -> u64 {
        let request = self.next_request;
        self.next_request = self.next_request.saturating_add(1);
        request
    }

    pub(crate) fn active_tab_ids(&self) -> Vec<TabId> {
        self.windows()
            .flat_map(Window::panes)
            .map(|pane| pane.active().id())
            .collect()
    }

    pub(crate) fn active_tabs_without_listing(&self) -> Vec<TabId> {
        self.windows()
            .flat_map(Window::panes)
            .map(Pane::active)
            .filter(|t| t.listing().is_none())
            .map(Tab::id)
            .collect()
    }

    pub(crate) fn unloaded_active_tabs(&self) -> Vec<TabId> {
        self.windows()
            .flat_map(Window::panes)
            .map(Pane::active)
            .filter(|t| t.listing().is_none() && t.pending_request().is_none())
            .map(Tab::id)
            .collect()
    }

    pub(crate) fn next_id(&self) -> u64 {
        self.next_id
    }

    pub(crate) fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane> {
        self.panes_mut().find(|p| p.id() == id)
    }

    #[must_use]
    pub fn tab(&self, id: TabId) -> Option<&Tab> {
        self.windows()
            .flat_map(Window::panes)
            .flat_map(Pane::tabs)
            .find(|tab| tab.id() == id)
    }

    pub(crate) fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.panes_mut().find_map(|p| p.tab_mut(id))
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

    pub(crate) fn close_pane_discarding_tabs(&mut self, pane: PaneId) -> Result<(), StateError> {
        self.focused.close_discarding_tabs(pane)
    }

    pub(crate) fn focus_direction(&mut self, direction: Direction) -> Result<PaneId, StateError> {
        self.focused.focus_direction(direction)
    }

    pub fn apply_tiles(&mut self, tiles: &Tiles) -> Applied {
        let location = self.active_tab().location().clone();
        let next_id = &mut self.next_id;
        self.focused.apply_tiles(tiles, &mut || {
            let pane = PaneId::new(*next_id);
            let tab = Tab::new(TabId::new(*next_id + 1), location.clone());
            *next_id += 2;
            Pane::new(pane, tab)
        })
    }

    pub fn swap(&mut self, a: PaneId, b: PaneId) -> Result<(), StateError> {
        self.focused.swap(a, b)
    }

    pub fn swap_toward(&mut self, direction: Direction) -> Result<(), StateError> {
        self.focused.swap_toward(direction)
    }

    pub fn resize(&mut self, direction: Direction, delta: u16) -> Result<(), StateError> {
        self.focused.resize(direction, delta)
    }

    pub fn set_ratio(&mut self, split: &SplitPath, ratio: u16) -> Result<(), StateError> {
        self.focused.set_ratio(split, ratio)
    }

    pub fn even(&mut self) {
        self.focused.even();
    }

    pub fn focus_number(&mut self, n: usize) -> Result<(), StateError> {
        self.focused.focus_number(n)
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

    fn panes_mut(&mut self) -> impl Iterator<Item = &mut Pane> {
        std::iter::once(&mut self.focused)
            .chain(self.others.values_mut())
            .flat_map(Window::panes_mut)
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
