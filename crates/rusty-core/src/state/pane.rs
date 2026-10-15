use super::{StateError, Tab};
use crate::PaneId;

// Invariant: `tabs` is never empty and `active < tabs.len()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    id: PaneId,
    tabs: Vec<Tab>,
    active: usize,
}

impl Pane {
    #[must_use]
    pub fn id(&self) -> PaneId {
        self.id
    }

    #[must_use]
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    #[must_use]
    pub fn active_index(&self) -> usize {
        self.active
    }

    #[must_use]
    pub fn active(&self) -> &Tab {
        &self.tabs[self.active]
    }

    pub(crate) fn new(id: PaneId, first: Tab) -> Self {
        Self {
            id,
            tabs: vec![first],
            active: 0,
        }
    }

    pub(crate) fn active_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    pub(crate) fn open_tab(&mut self, tab: Tab) {
        self.active += 1;
        self.tabs.insert(self.active, tab);
    }

    pub(crate) fn close_tab(&mut self, index: usize) -> Result<(), StateError> {
        if index >= self.tabs.len() {
            return Err(StateError::NoSuchTab(index));
        }
        if self.tabs.len() == 1 {
            return Err(StateError::LastTab);
        }
        self.tabs.remove(index);
        if index < self.active || self.active == self.tabs.len() {
            self.active -= 1;
        }
        Ok(())
    }

    pub(crate) fn activate(&mut self, index: usize) -> Result<(), StateError> {
        if index >= self.tabs.len() {
            return Err(StateError::NoSuchTab(index));
        }
        self.active = index;
        Ok(())
    }
}
