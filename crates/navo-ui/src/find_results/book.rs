//! Multi-tab state, event draining, global memory budget and session persistence for find results.

use std::time::Instant;

use navo_core::{Location, SavedFind};

use super::tab::DrainStep;
pub use super::tab::{FindTab, Notice, ROW_REBUILD_INTERVAL, UnreachableRoot};

/// Maximum number of open search tabs in the results window.
pub const MAX_TABS: usize = 12;

/// Global hit-store memory budget shared across every open tab (48 MiB).
pub const MEMORY_BUDGET: usize = 48 * 1024 * 1024;

/// Hit lines kept per file when trimming for memory or on window close.
pub const TRIM_LINES: usize = 3;

/// Maximum number of run events drained per call.
pub const MAX_EVENTS_PER_DRAIN: usize = 16;

/// Collection of open find-results tabs and their shared memory budget.
pub struct FindBook {
    tabs: Vec<FindTab>,
    active: usize,
    next_id: u64,
    budget: usize,
}

impl Default for FindBook {
    fn default() -> Self {
        Self::new()
    }
}

impl FindBook {
    /// Creates an empty book using the global 48 MiB memory budget.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            budget: MEMORY_BUDGET,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_budget(budget: usize) -> Self {
        Self {
            budget,
            ..Self::new()
        }
    }

    /// Number of open tabs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    /// Whether no search tab is open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    /// Open tabs in creation order (oldest first).
    #[must_use]
    pub fn tabs(&self) -> &[FindTab] {
        &self.tabs
    }

    /// Index of the active tab, or `0` when the book is empty.
    #[must_use]
    pub fn active_index(&self) -> usize {
        self.active
    }

    /// The active tab, if any.
    #[must_use]
    pub fn active(&self) -> Option<&FindTab> {
        self.tabs.get(self.active)
    }

    /// Mutable reference to the active tab, if any.
    pub fn active_mut(&mut self) -> Option<&mut FindTab> {
        self.tabs.get_mut(self.active)
    }

    /// Looks up a tab by id.
    #[must_use]
    pub fn tab(&self, id: u64) -> Option<&FindTab> {
        self.tabs.iter().find(|tab| tab.id == id)
    }

    /// Looks up a mutable tab by id.
    pub fn tab_mut(&mut self, id: u64) -> Option<&mut FindTab> {
        self.tabs.iter_mut().find(|tab| tab.id == id)
    }

    /// Activates the tab with `id`, returning whether it exists.
    pub fn select(&mut self, id: u64) -> bool {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return false;
        };
        self.active = index;
        true
    }

    /// Adds a new tab, evicting the oldest finished tab (or oldest overall) past [`MAX_TABS`].
    pub fn add(&mut self, text: impl Into<String>, base: Location) -> u64 {
        if self.tabs.len() >= MAX_TABS {
            let victim = self
                .tabs
                .iter()
                .position(|tab| !tab.is_running())
                .unwrap_or(0);
            self.tabs.remove(victim);
        }
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.tabs.push(FindTab::new(id, text.into(), base));
        self.active = self.tabs.len() - 1;
        id
    }

    /// Closes the tab with `id`, keeping the active selection valid.
    pub fn close(&mut self, id: u64) -> bool {
        let Some(index) = self.tabs.iter().position(|tab| tab.id == id) else {
            return false;
        };
        self.tabs.remove(index);
        if self.tabs.is_empty() {
            self.active = 0;
        } else if index < self.active {
            self.active -= 1;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        true
    }

    /// Switches to the next tab, wrapping around.
    pub fn next(&mut self) {
        if self.tabs.len() > 1 {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    /// Switches to the previous tab, wrapping around.
    pub fn previous(&mut self) {
        if self.tabs.len() > 1 {
            self.active = if self.active == 0 {
                self.tabs.len() - 1
            } else {
                self.active - 1
            };
        }
    }

    /// Drains at most [`MAX_EVENTS_PER_DRAIN`] run events and refreshes rows on cadence.
    pub fn drain(&mut self, now: Instant) -> bool {
        let mut changed = self.drain_events(now);
        for tab in &mut self.tabs {
            if tab.refresh_rows_if_due(now) {
                changed = true;
            }
        }
        changed
    }

    /// Enforces the global memory budget across every open tab.
    ///
    /// Over budget, finished tabs are trimmed oldest-first to [`TRIM_LINES`] lines per file. If
    /// the budget is still exceeded afterward, running tabs stop storing new hit lines.
    pub fn enforce_budget(&mut self) {
        if self.memory_bytes() <= self.budget {
            return;
        }
        for index in 0..self.tabs.len() {
            if self.tabs[index].is_running() || !self.tabs[index].has_lines_over(TRIM_LINES) {
                continue;
            }
            self.tabs[index].trim_lines(TRIM_LINES, true);
            if self.memory_bytes() <= self.budget {
                return;
            }
        }
        if self.memory_bytes() <= self.budget {
            return;
        }
        for tab in &mut self.tabs {
            if tab.is_running() && tab.store.stopped().is_none() {
                tab.store.stop_lines();
                tab.notice = Some(Notice::LinesStopped);
            }
        }
    }

    /// Trims stored hit lines in every open tab to at most `keep` lines per file.
    pub fn trim_all(&mut self, keep: usize) {
        for tab in &mut self.tabs {
            tab.trim_lines(keep, false);
        }
    }

    /// Queries to persist across sessions (non-blank tabs, oldest first).
    #[must_use]
    pub fn saved(&self) -> Vec<SavedFind> {
        self.tabs
            .iter()
            .filter(|tab| !tab.text.trim().is_empty())
            .map(|tab| SavedFind {
                text: tab.text.clone(),
                base: tab.base.clone(),
            })
            .collect()
    }

    /// Restores saved queries as idle tabs without starting any search run.
    pub fn restore(&mut self, saved: Vec<SavedFind>) {
        self.tabs.clear();
        self.active = 0;
        for item in saved {
            if !item.text.trim().is_empty() {
                self.add(item.text, item.base);
            }
        }
    }

    /// Total hit-store memory across every open tab.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        self.tabs
            .iter()
            .map(|tab| tab.store.memory_bytes())
            .fold(0, usize::saturating_add)
    }

    fn drain_events(&mut self, now: Instant) -> bool {
        if self.tabs.is_empty() {
            return false;
        }
        let mut changed = false;
        let mut drained = 0usize;
        let mut idle_streak = 0usize;
        let mut cursor = self.active.min(self.tabs.len() - 1);
        while drained < MAX_EVENTS_PER_DRAIN && idle_streak < self.tabs.len() {
            let step = self.tabs[cursor].drain_one(now);
            cursor = (cursor + 1) % self.tabs.len();
            let DrainStep::Event { pushed } = step else {
                idle_streak += 1;
                continue;
            };
            changed = true;
            drained += 1;
            idle_streak = 0;
            if pushed {
                self.enforce_budget();
            }
        }
        changed
    }
}
