use super::{StateError, Tab};
use crate::{PaneId, TabId};

const DEFAULT_PAGE_ROWS: u64 = 20;

// Invariant: `tabs` is never empty and `active < tabs.len()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    id: PaneId,
    tabs: Vec<Tab>,
    active: usize,
    page_rows: u64,
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

    #[must_use]
    pub fn page_rows(&self) -> u64 {
        self.page_rows
    }

    pub(crate) fn set_page_rows(&mut self, rows: u64) {
        self.page_rows = rows.max(1);
    }

    pub(crate) fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id() == id)
    }

    pub(crate) fn restore(
        id: PaneId,
        tabs: Vec<Tab>,
        active: usize,
        page_rows: u64,
    ) -> Result<Self, String> {
        if tabs.is_empty() {
            return Err(format!("pane {} has no tabs", id.get()));
        }
        if active >= tabs.len() {
            return Err(format!("pane {} has no tab at index {active}", id.get()));
        }
        Ok(Self {
            id,
            tabs,
            active,
            page_rows: page_rows.max(1),
        })
    }

    pub(crate) fn new(id: PaneId, first: Tab) -> Self {
        Self {
            id,
            tabs: vec![first],
            active: 0,
            page_rows: DEFAULT_PAGE_ROWS,
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

    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the tab-activate command arrives with P7")
    )]
    pub(crate) fn activate(&mut self, index: usize) -> Result<(), StateError> {
        if index >= self.tabs.len() {
            return Err(StateError::NoSuchTab(index));
        }
        self.active = index;
        Ok(())
    }
}
