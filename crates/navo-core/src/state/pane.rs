use super::{StateError, Tab};
use crate::{PaneId, TabId};

const DEFAULT_PAGE_ROWS: u64 = 20;

// Invariant: `tabs` is never empty and `active < tabs.len()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    pub(super) id: PaneId,
    pub(super) tabs: Vec<Tab>,
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

    pub(super) fn workspace_snapshot(&self) -> Self {
        Self {
            id: self.id,
            tabs: self.tabs.iter().map(Tab::workspace_snapshot).collect(),
            active: self.active,
            page_rows: self.page_rows,
        }
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
        let mut pane = Self {
            id,
            tabs,
            active,
            page_rows: page_rows.max(1),
        };
        pane.keep_pinned_first();
        Ok(pane)
    }

    pub(crate) fn new(id: PaneId, first: Tab) -> Self {
        Self {
            id,
            tabs: vec![first],
            active: 0,
            page_rows: DEFAULT_PAGE_ROWS,
        }
    }

    pub(crate) fn reveal_cursor(&mut self) {
        let page = self.page_rows.max(1);
        let tab = self.active_mut();
        let (cursor, scroll) = (tab.view().cursor, tab.view().scroll);
        if cursor < scroll {
            tab.set_scroll(cursor);
        } else if cursor >= scroll.saturating_add(page) {
            tab.set_scroll(cursor + 1 - page);
        }
    }

    pub(crate) fn active_mut(&mut self) -> &mut Tab {
        &mut self.tabs[self.active]
    }

    pub(crate) fn open_tab(&mut self, tab: Tab) {
        self.active += 1;
        self.tabs.insert(self.active, tab);
        self.keep_pinned_first();
    }

    pub(super) fn take_tabs(self) -> Vec<Tab> {
        self.tabs
    }

    pub(super) fn append_tabs(&mut self, tabs: Vec<Tab>) {
        self.tabs.extend(tabs);
        self.keep_pinned_first();
    }

    #[cfg(test)]
    pub(crate) fn close_tab(&mut self, index: usize) -> Result<(), StateError> {
        self.take_tab(index).map(drop)
    }

    pub(super) fn take_tab(&mut self, index: usize) -> Result<Tab, StateError> {
        if index >= self.tabs.len() {
            return Err(StateError::NoSuchTab(index));
        }
        if self.tabs.len() == 1 {
            return Err(StateError::LastTab);
        }
        let tab = self.tabs.remove(index);
        if index < self.active || self.active == self.tabs.len() {
            self.active -= 1;
        }
        Ok(tab)
    }

    pub(super) fn insert_tab(&mut self, at: usize, tab: Tab) {
        let at = at.min(self.tabs.len());
        self.tabs.insert(at, tab);
        self.active = at;
        self.keep_pinned_first();
    }

    #[must_use]
    pub fn pinned_count(&self) -> usize {
        self.tabs.iter().take_while(|tab| tab.is_pinned()).count()
    }

    pub(super) fn keep_pinned_first(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let active = self.tabs[self.active].id();
        let (mut tabs, unpinned): (Vec<Tab>, Vec<Tab>) = std::mem::take(&mut self.tabs)
            .into_iter()
            .partition(Tab::is_pinned);
        tabs.extend(unpinned);
        self.tabs = tabs;
        self.active = self.position(active).unwrap_or(0);
    }

    pub(super) fn position(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id() == id)
    }

    pub(crate) fn activate(&mut self, index: usize) -> Result<(), StateError> {
        if index >= self.tabs.len() {
            return Err(StateError::NoSuchTab(index));
        }
        self.active = index;
        Ok(())
    }
}
