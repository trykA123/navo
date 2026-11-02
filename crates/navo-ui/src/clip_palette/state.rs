use std::collections::HashSet;
use std::rc::Rc;

use super::{ClipId, ClipKind, ClipMode, ClipResults, ClipRow, Origin};

/// One line of the list: a group head in All, or a row by index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Entry {
    Head(Origin),
    Row(usize),
}

/// What the palette shows: the offered modes, the latest accepted answer and the selection.
pub(super) struct PaletteState {
    modes: Vec<ClipMode>,
    mode: ClipMode,
    results: Rc<ClipResults>,
    entries: Rc<[Entry]>,
    counts: Vec<(ClipMode, usize)>,
    selected: usize,
    request: u64,
    keep: Option<ClipId>,
    with_user: HashSet<ClipId>,
    asked: ClipMode,
    shown: ClipMode,
    is_loaded: bool,
}

impl PaletteState {
    pub fn new(modes: Vec<ClipMode>) -> Self {
        let mode = if modes.is_empty() || modes.contains(&ClipMode::All) {
            ClipMode::All
        } else {
            modes[0]
        };
        Self {
            modes,
            mode,
            results: Rc::default(),
            entries: Rc::default(),
            counts: Vec::new(),
            selected: 0,
            request: 0,
            keep: None,
            with_user: HashSet::new(),
            asked: mode,
            shown: mode,
            is_loaded: false,
        }
    }

    pub fn modes(&self) -> &[ClipMode] {
        &self.modes
    }

    pub fn mode(&self) -> ClipMode {
        self.mode
    }

    /// The mode the shown rows answer, which lags `mode` until the new answer arrives.
    pub fn shown(&self) -> ClipMode {
        self.shown
    }

    pub fn results(&self) -> &Rc<ClipResults> {
        &self.results
    }

    /// The list in order, with group heads when All mixes history and saved rows.
    pub fn entries(&self) -> &Rc<[Entry]> {
        &self.entries
    }

    /// Where the selected row sits among the entries.
    pub fn selected_entry(&self) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| *entry == Entry::Row(self.selected))
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn selected_row(&self) -> Option<&ClipRow> {
        self.results.rows.get(self.selected)
    }

    /// Whether a secret row had a user name when it arrived; the name itself is not kept.
    pub fn has_user(&self, id: ClipId) -> bool {
        self.with_user.contains(&id)
    }

    /// False until the first answer arrives, so the empty state never flashes.
    pub fn is_loaded(&self) -> bool {
        self.is_loaded
    }

    pub fn count_of(&self, mode: ClipMode) -> Option<usize> {
        if mode == self.shown {
            return self.is_loaded.then_some(self.results.rows.len());
        }
        self.counts
            .iter()
            .find_map(|(shown, count)| (*shown == mode).then_some(*count))
    }

    /// Moves to the next or previous offered mode; false when there is nowhere to go.
    pub fn cycle(&mut self, backwards: bool) -> bool {
        let Some(at) = self.modes.iter().position(|mode| *mode == self.mode) else {
            return false;
        };
        let count = self.modes.len();
        let to = if backwards {
            (at + count - 1) % count
        } else {
            (at + 1) % count
        };
        self.set_mode(self.modes[to])
    }

    /// Picks an offered mode; false when it is not offered or already shown.
    pub fn set_mode(&mut self, mode: ClipMode) -> bool {
        if mode == self.mode || !self.modes.contains(&mode) {
            return false;
        }
        self.mode = mode;
        self.selected = 0;
        true
    }

    /// Starts a request; answers to any earlier one are dropped. A refresh of the same
    /// search keeps the selected row, a new search starts at the top.
    pub fn begin_request(&mut self, is_refresh: bool) -> u64 {
        self.keep = is_refresh
            .then(|| self.selected_row().map(|row| row.id))
            .flatten();
        if !is_refresh {
            self.selected = 0;
        }
        self.asked = self.mode;
        self.request += 1;
        self.request
    }

    /// Drops every row and count, and any answer still on its way.
    pub fn clear(&mut self) {
        self.results = Rc::default();
        self.entries = Rc::default();
        self.counts.clear();
        self.with_user.clear();
        self.selected = 0;
        self.keep = None;
        self.is_loaded = false;
        self.request += 1;
    }

    /// Takes an answer unless a newer request exists or its revision is older than the shown one.
    pub fn accept(&mut self, request: u64, mut results: ClipResults) -> bool {
        if request != self.request || results.revision < self.results.revision {
            return false;
        }
        self.with_user.clear();
        for row in results
            .rows
            .iter_mut()
            .filter(|row| row.kind == ClipKind::Secret)
        {
            if !row.detail.is_empty() {
                self.with_user.insert(row.id);
            }
            row.detail.clear();
        }
        self.results = Rc::new(results);
        self.is_loaded = true;
        self.shown = self.asked;
        self.entries = self.build_entries();
        self.selected = self
            .keep
            .take()
            .and_then(|id| self.position_of(id))
            .unwrap_or(self.selected)
            .min(self.results.rows.len().saturating_sub(1));
        true
    }

    pub fn accept_counts(&mut self, request: u64, counts: Vec<(ClipMode, usize)>) -> bool {
        if request != self.request {
            return false;
        }
        self.counts = counts;
        true
    }

    pub fn step(&mut self, delta: isize) {
        let last = self.results.rows.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(delta).min(last);
    }

    pub fn select(&mut self, index: usize) {
        if index < self.results.rows.len() {
            self.selected = index;
        }
    }

    fn build_entries(&self) -> Rc<[Entry]> {
        let rows = &self.results.rows;
        let mut entries = Vec::with_capacity(rows.len() + 2);
        let mut last = None;
        for (index, row) in rows.iter().enumerate() {
            if self.shown == ClipMode::All && last != Some(row.from) {
                entries.push(Entry::Head(row.from));
                last = Some(row.from);
            }
            entries.push(Entry::Row(index));
        }
        entries.into()
    }

    fn position_of(&self, id: ClipId) -> Option<usize> {
        self.results.rows.iter().position(|row| row.id == id)
    }
}
