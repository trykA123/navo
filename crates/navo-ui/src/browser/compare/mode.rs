use navo_core::{CursorMove, Location, PaneId, ProviderPath};

use super::CompareUpdate;
use super::hooks::{CompareControl, CompareRow, CompareTotals, RowProgress, RowStatus};
use super::rows::{Applied, CompareRows};

/// Which rows the panes list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CompareFilter {
    #[default]
    All,
    Differences,
    Orphans,
    Same,
}

impl CompareFilter {
    /// Checking, not-checked and unavailable rows show under `All` only.
    #[must_use]
    pub fn keeps(self, row: &CompareRow) -> bool {
        let status = row.status;
        match self {
            Self::All => true,
            Self::Differences => row.is_difference(),
            Self::Orphans => matches!(status, RowStatus::OnlyLeft | RowStatus::OnlyRight),
            Self::Same => status.is_same(),
        }
    }
}

/// Rows a Page Up or Page Down key moves the selection by.
const PAGE: usize = 10;

/// A pane as it was before the compare started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Saved {
    pub pane: PaneId,
    pub location: Location,
    pub cursor: Option<String>,
}

/// Enter on a file row: the file diff (U7) opens these sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDiffRequest {
    pub path: String,
    pub left: Option<Location>,
    pub right: Option<Location>,
}

/// One step of the Esc chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExitStep {
    FilterCleared,
    Leave,
}

/// A folder compare across the two panes. Index 0 is the left side.
pub struct CompareMode {
    pub(super) run: u64,
    pub(super) control: Box<dyn CompareControl>,
    pub(super) roots: [Location; 2],
    pub(super) saved: [Saved; 2],
    pub(super) focused: PaneId,
    pub(super) failed: Option<String>,
    rows: CompareRows,
    folder: String,
    filter: CompareFilter,
    selected: Option<String>,
    revision: u64,
}

impl CompareMode {
    pub(super) fn new(
        run: u64,
        control: Box<dyn CompareControl>,
        saved: [Saved; 2],
        focused: PaneId,
    ) -> Self {
        let roots = [saved[0].location.clone(), saved[1].location.clone()];
        Self {
            run,
            control,
            roots,
            saved,
            focused,
            failed: None,
            rows: CompareRows::new(),
            folder: String::new(),
            filter: CompareFilter::All,
            selected: None,
            revision: 0,
        }
    }

    /// Applies one update; returns whether anything changed.
    pub(super) fn apply(&mut self, update: CompareUpdate) -> bool {
        let generation = self.rows.generation();
        let listed = self.touches_folder(&update);
        let applied = self.rows.apply(update);
        if let Applied::Failed(text) = applied {
            self.failed = Some(text);
            return true;
        }
        let changed = applied == Applied::Changed;
        let relisted = self.rows.generation() != generation;
        self.revision += u64::from(changed && (listed || relisted));
        changed
    }

    /// Whether the update brings rows or progress of the folder shown.
    fn touches_folder(&self, update: &CompareUpdate) -> bool {
        let CompareUpdate::Rows { rows, progress, .. } = update else {
            return false;
        };
        rows.iter().any(|row| row.parent() == self.folder)
            || progress
                .iter()
                .any(|step| self.rows.parent_of(step.id) == Some(self.folder.as_str()))
    }

    /// Changes whenever the rows listed now or the filter change, so a view can cache what it
    /// built; totals, the stop notice and rows of other folders leave it alone.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn run(&self) -> u64 {
        self.run
    }

    /// The rows the panes list now, in this order: the current folder under the filter.
    pub fn visible(&self) -> Vec<&CompareRow> {
        self.rows
            .children(&self.folder)
            .filter(|row| self.filter.keeps(row))
            .collect()
    }

    /// The folder shown, relative to the roots; `""` is the roots.
    #[must_use]
    pub fn folder(&self) -> &str {
        &self.folder
    }

    #[must_use]
    pub fn roots(&self) -> &[Location; 2] {
        &self.roots
    }

    /// Whether `pane` is one of the two compared panes.
    #[must_use]
    pub fn has_pane(&self, pane: PaneId) -> bool {
        self.side_of(pane).is_some()
    }

    /// 0 for the left pane, 1 for the right one.
    #[must_use]
    pub fn side_of(&self, pane: PaneId) -> Option<usize> {
        self.saved.iter().position(|saved| saved.pane == pane)
    }

    #[must_use]
    pub fn filter(&self) -> CompareFilter {
        self.filter
    }

    /// Sets the filter; a selection the filter hides moves to the first row left.
    pub fn set_filter(&mut self, filter: CompareFilter) {
        self.filter = filter;
        self.revision += 1;
        let visible = self.visible();
        if self.selected_at(&visible).is_none() {
            self.selected = visible.first().map(|row| row.path.clone());
        }
    }

    /// The selected row; it survives a restart when a row with its path comes back.
    #[must_use]
    pub fn selected(&self) -> Option<&CompareRow> {
        self.rows.by_path(self.selected.as_deref()?)
    }

    pub fn select(&mut self, path: &str) {
        self.selected = Some(path.to_owned());
    }

    /// The path of the selection, even while its row has not arrived.
    #[must_use]
    pub fn selected_path(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// Moves the selection like a list cursor; with nothing selected it lands on the first row.
    pub fn move_cursor(&mut self, to: CursorMove) -> bool {
        let visible = self.visible();
        let Some(last) = visible.len().checked_sub(1) else {
            return false;
        };
        let target = match (to, self.selected_at(&visible)) {
            (CursorMove::First, _) | (_, None) => 0,
            (CursorMove::Last, _) => last,
            (CursorMove::Up, Some(at)) => at.saturating_sub(1),
            (CursorMove::Down, Some(at)) => (at + 1).min(last),
            (CursorMove::PageUp, Some(at)) => at.saturating_sub(PAGE),
            (CursorMove::PageDown, Some(at)) => (at + PAGE).min(last),
            (CursorMove::Row(row), Some(_)) => usize::try_from(row).map_or(last, |r| r.min(last)),
        };
        let path = visible.get(target).map(|row| row.path.clone());
        self.move_to(path)
    }

    #[must_use]
    pub fn totals(&self) -> CompareTotals {
        self.rows.totals()
    }

    /// True until the newest generation finished.
    #[must_use]
    pub fn checking(&self) -> bool {
        self.rows.checking()
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.rows.generation()
    }

    #[must_use]
    pub fn progress(&self, id: u64) -> Option<RowProgress> {
        self.rows.progress(id)
    }

    /// Entries listed so far while the walk waits at the network limit; `None` when it runs.
    #[must_use]
    pub fn stopped(&self) -> Option<u64> {
        self.rows.stopped()
    }

    /// Asks a walk that waits at the network limit to go on; false when none waits.
    pub fn continue_walk(&self) -> bool {
        let waiting = self.rows.stopped().is_some();
        if waiting {
            self.control.continue_walk();
        }
        waiting
    }

    #[must_use]
    pub fn received(&self) -> usize {
        self.rows.len()
    }

    /// Moves to the next difference below the selection; stops at the last one.
    /// A folder that holds a difference counts: the move lands on it and Enter goes in.
    pub(super) fn next_difference(&mut self) -> bool {
        let visible = self.visible();
        let start = self.selected_at(&visible).map_or(0, |at| at + 1);
        let found = visible.iter().skip(start).find(|row| row.is_difference());
        self.move_to(found.map(|row| row.path.clone()))
    }

    /// Moves to the previous difference above the selection; stops at the first one.
    pub(super) fn previous_difference(&mut self) -> bool {
        let visible = self.visible();
        let end = self.selected_at(&visible).unwrap_or(visible.len());
        let found = visible
            .iter()
            .take(end)
            .rev()
            .find(|row| row.is_difference());
        self.move_to(found.map(|row| row.path.clone()))
    }

    fn selected_at(&self, visible: &[&CompareRow]) -> Option<usize> {
        let selected = self.selected.as_deref()?;
        visible.iter().position(|row| row.path == selected)
    }

    fn move_to(&mut self, path: Option<String>) -> bool {
        let moved = path.is_some();
        if moved {
            self.selected = path;
        }
        moved
    }

    pub(super) fn exit_step(&mut self) -> ExitStep {
        if self.filter == CompareFilter::All {
            return ExitStep::Leave;
        }
        self.set_filter(CompareFilter::All);
        ExitStep::FilterCleared
    }

    /// Shows `folder` and selects nothing in it yet.
    pub(super) fn show_folder(&mut self, folder: String) {
        self.folder = folder;
        self.selected = None;
        self.revision += 1;
    }

    /// Where `path` is on one side, or `None` when a part cannot be joined.
    pub(super) fn side_location(&self, side: usize, path: &str) -> Option<Location> {
        let root = self.roots.get(side)?.clone();
        path.split('/')
            .filter(|part| !part.is_empty())
            .try_fold(root, |at, part| {
                at.join(&ProviderPath::from_text(part)).ok()
            })
    }

    pub(super) fn file_diff(&self, row: &CompareRow) -> FileDiffRequest {
        let file = |side: Option<super::CompareSide>| {
            side.is_some_and(|s| s.kind == super::CompareKind::File)
        };
        FileDiffRequest {
            path: row.path.clone(),
            left: file(row.left)
                .then(|| self.side_location(0, &row.path))
                .flatten(),
            right: file(row.right)
                .then(|| self.side_location(1, &row.path))
                .flatten(),
        }
    }
}
