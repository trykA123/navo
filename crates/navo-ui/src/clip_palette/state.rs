use std::collections::HashSet;
use std::rc::Rc;

use navo_core::Location;

use super::notes::{is_folder_row, is_placeholder};
use super::{ClipId, ClipKind, ClipMode, ClipResults, ClipRow, Origin};

/// The two group heads of Notes mode when the active folder's note leads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NotesHead {
    ThisFolder,
    Others,
}

/// One line of the list: a group head in All or Notes, or a row by index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Entry {
    Head(Origin),
    NotesHead(NotesHead),
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
    /// The folder whose note leads Notes mode; none in the pop-up.
    folder: Option<Location>,
    has_folder_note: bool,
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
            folder: None,
            has_folder_note: false,
        }
    }

    pub fn with_folder(mut self, folder: Option<Location>) -> Self {
        self.folder = folder;
        self
    }

    pub fn folder(&self) -> Option<&Location> {
        self.folder.as_ref()
    }

    /// The mode the latest request asked for, which the next accepted answer belongs to.
    pub fn asked(&self) -> ClipMode {
        self.asked
    }

    /// Rows that stand for stored items: the placeholder of an empty folder note is none.
    pub fn item_count(&self) -> usize {
        let rows = &self.results.rows;
        rows.len() - usize::from(self.has_folder_note && rows.first().is_some_and(is_placeholder))
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
            return self.is_loaded.then(|| self.item_count());
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
        self.has_folder_note = false;
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
        self.has_folder_note = self.shown == ClipMode::Notes
            && self.folder.as_ref().is_some_and(|folder| {
                self.results
                    .rows
                    .first()
                    .is_some_and(|row| is_folder_row(row, folder))
            });
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
        if self.has_folder_note {
            entries.push(Entry::NotesHead(NotesHead::ThisFolder));
            entries.push(Entry::Row(0));
            if rows.len() > 1 {
                entries.push(Entry::NotesHead(NotesHead::Others));
            }
            entries.extend((1..rows.len()).map(Entry::Row));
            return entries.into();
        }
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
