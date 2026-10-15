mod history;
mod listing;
mod pane;
mod tab;

use std::collections::BTreeMap;

use crate::{Direction, Layout, LayoutError, Location, PaneId, WindowId};
pub use history::History;
pub use listing::{Entry, Listing};
pub use pane::Pane;
pub use tab::{Tab, ViewState};

#[allow(dead_code, reason = "stubs until the state is implemented")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    id: WindowId,
    layout: Layout,
    panes: BTreeMap<PaneId, Pane>,
}

#[allow(dead_code, reason = "stubs until the state is implemented")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppState {
    windows: BTreeMap<WindowId, Window>,
    focused: WindowId,
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

#[allow(unused_variables, reason = "stubs until the state is implemented")]
impl Window {
    #[must_use]
    pub fn id(&self) -> WindowId {
        todo!()
    }

    #[must_use]
    pub fn layout(&self) -> &Layout {
        todo!()
    }

    #[must_use]
    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        todo!()
    }

    #[must_use]
    pub fn panes(&self) -> Vec<&Pane> {
        todo!()
    }
}

#[allow(
    unused_variables,
    dead_code,
    reason = "stubs until the state is implemented"
)]
impl AppState {
    #[must_use]
    pub fn new(home: Location) -> Self {
        todo!()
    }

    #[must_use]
    pub fn window_ids(&self) -> Vec<WindowId> {
        todo!()
    }

    #[must_use]
    pub fn focused_window(&self) -> WindowId {
        todo!()
    }

    #[must_use]
    pub fn window(&self, id: WindowId) -> Option<&Window> {
        todo!()
    }

    #[must_use]
    pub fn focused_pane(&self) -> PaneId {
        todo!()
    }

    #[must_use]
    pub fn pane(&self, id: PaneId) -> Option<&Pane> {
        todo!()
    }

    #[must_use]
    pub fn active_tab(&self) -> &Tab {
        todo!()
    }

    pub(crate) fn active_tab_mut(&mut self) -> &mut Tab {
        todo!()
    }

    pub(crate) fn focused_pane_mut(&mut self) -> &mut Pane {
        todo!()
    }

    pub(crate) fn new_tab(&mut self, location: Location) -> Tab {
        todo!()
    }

    pub(crate) fn split(&mut self, direction: Direction) -> Result<PaneId, StateError> {
        todo!()
    }

    pub(crate) fn close_pane(&mut self, pane: PaneId) -> Result<(), StateError> {
        todo!()
    }

    pub(crate) fn focus_direction(&mut self, direction: Direction) -> Result<PaneId, StateError> {
        todo!()
    }

    pub(crate) fn open_window(&mut self, location: Location) -> WindowId {
        todo!()
    }

    pub(crate) fn close_window(&mut self, id: WindowId) -> Result<(), StateError> {
        todo!()
    }

    pub(crate) fn focus_window(&mut self, id: WindowId) -> Result<(), StateError> {
        todo!()
    }
}

#[cfg(test)]
mod tests;
