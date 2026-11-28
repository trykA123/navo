//! Single-tab state for the find results book: hit store, rows, cursor and run events.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use navo_core::Location;
use navo_core::find::FindQuery;
use navo_fs::find::{FindError, FindProgress, FoundBatch, search_root};

use crate::command_center::content_source::ContentWalk;

use super::rows::{self, LINES_PER_FILE, LinesShown, Row, RowMode};
use super::run::{self, FindRun, RunEvent};
use super::scroll::Scroll;
use super::store::{HitStore, StoreError};

/// Minimum delay between live row rebuilds while a search is running.
pub const ROW_REBUILD_INTERVAL: Duration = Duration::from_millis(100);

/// Outcome of polling one event from a tab's active run.
pub(super) enum DrainStep {
    /// One event (or disconnect) was processed; `pushed` is true when hits entered the store.
    Event { pushed: bool },
    /// No event was ready on this tab.
    Idle,
}

/// Non-fatal condition shown above a tab's results.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// Stored hit lines were trimmed to keep global memory within budget.
    Trimmed,
    /// Hit line storage stopped for this run; file and hit counts continue.
    LinesStopped,
    /// The text arena hit its offset ceiling and refused further batches.
    OffsetsFull,
}

/// A search root that failed to resolve or could not be walked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnreachableRoot {
    /// A resolved root whose walk failed.
    WalkFailed { location: Location, error: String },
    /// A root that never resolved to a `Location` (`root >= roots.len()`).
    Unresolved { within: String, error: String },
}

impl UnreachableRoot {
    /// Resolved root location, if the root resolved before failing.
    #[must_use]
    pub fn location(&self) -> Option<&Location> {
        match self {
            Self::WalkFailed { location, .. } => Some(location),
            Self::Unresolved { .. } => None,
        }
    }

    /// Text identifying the root: the location path, or the raw `in:` text when unresolved.
    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::WalkFailed { location, .. } => location.as_str(),
            Self::Unresolved { within, .. } => within,
        }
    }

    /// Error message reported by the run.
    #[must_use]
    pub fn error(&self) -> &str {
        match self {
            Self::WalkFailed { error, .. } | Self::Unresolved { error, .. } => error,
        }
    }
}

impl fmt::Display for UnreachableRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WalkFailed { location, error } => write!(f, "{}: {error}", location.as_str()),
            Self::Unresolved { within, error } if !within.is_empty() => {
                write!(f, "{within}: {error}")
            }
            Self::Unresolved { error, .. } => f.write_str(error),
        }
    }
}

/// One search tab inside [`super::book::FindBook`].
pub struct FindTab {
    pub id: u64,
    pub text: String,
    pub base: Location,
    pub query: FindQuery,
    pub roots: Vec<Location>,
    pub store: HitStore,
    pub rows: Vec<Row>,
    pub mode: RowMode,
    pub scroll: Scroll,
    pub cursor: Option<Row>,
    pub marks: BTreeSet<u32>,
    pub run: Option<FindRun>,
    pub progress: FindProgress,
    pub unreachable: Vec<UnreachableRoot>,
    pub error: Option<String>,
    pub notice: Option<Notice>,
    rows_built: Option<Instant>,
    dirty: bool,
    progress_base: FindProgress,
    progress_root: Option<u32>,
    root_progress: FindProgress,
}

impl FindTab {
    /// Creates an idle tab for `text` starting at `base`.
    #[must_use]
    pub fn new(id: u64, text: String, base: Location) -> Self {
        let query = FindQuery::parse(&text);
        Self {
            id,
            text,
            base,
            query,
            roots: Vec::new(),
            store: HitStore::default(),
            rows: Vec::new(),
            mode: RowMode {
                grouped: true,
                lines: LinesShown::Auto,
                sort: None,
            },
            scroll: Scroll::default(),
            cursor: None,
            marks: BTreeSet::new(),
            run: None,
            progress: FindProgress::default(),
            unreachable: Vec::new(),
            error: None,
            notice: None,
            rows_built: None,
            dirty: false,
            progress_base: FindProgress::default(),
            progress_root: None,
            root_progress: FindProgress::default(),
        }
    }

    /// Whether this tab currently owns a live background search run.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.run.is_some()
    }

    /// Resets results and starts a background walk for the tab's current query and base.
    pub fn start(&mut self, walk: ContentWalk) {
        self.run = None;
        self.query = FindQuery::parse(&self.text);
        self.store = HitStore::default();
        self.rows.clear();
        self.scroll.top = 0;
        self.cursor = None;
        self.marks.clear();
        self.progress = FindProgress::default();
        self.progress_base = FindProgress::default();
        self.progress_root = None;
        self.root_progress = FindProgress::default();
        self.unreachable.clear();
        self.error = None;
        self.notice = None;
        self.rows_built = None;
        self.dirty = false;
        let (run, roots) = run::start(walk, self.base.clone(), self.query.clone());
        self.roots = roots;
        self.run = Some(run);
    }

    /// Cancels any live background walk and releases the folder interning index.
    pub fn stop(&mut self) {
        if self.run.take().is_some() {
            self.store.finish_index();
        }
    }

    /// Current position of `cursor` in `rows`, if present.
    #[must_use]
    pub fn cursor_index(&self) -> Option<usize> {
        self.cursor.and_then(|row| rows::index_of(&self.rows, row))
    }

    /// Rebuilds visible rows from the store, keeping the cursor by row identity.
    pub fn rebuild_rows(&mut self, now: Instant) {
        self.rebuild_rows_now();
        self.rows_built = Some(now);
    }

    pub(super) fn drain_one(&mut self, now: Instant) -> DrainStep {
        let Some(run) = self.run.as_ref() else {
            return DrainStep::Idle;
        };
        match run.receive.try_recv() {
            Ok(RunEvent::Batch { root, batch }) => {
                let pushed = self.apply_batch(root, batch);
                DrainStep::Event { pushed }
            }
            Ok(RunEvent::RootFailed { root, error }) => {
                self.record_root_failed(root, error);
                DrainStep::Event { pushed: false }
            }
            Ok(RunEvent::Done(result)) => {
                self.finish_run(result, now);
                DrainStep::Event { pushed: false }
            }
            Err(TryRecvError::Disconnected) => {
                let message = self
                    .error
                    .take()
                    .unwrap_or_else(|| FindError::WorkerStopped.to_string());
                self.finish_run(Err(message), now);
                DrainStep::Event { pushed: false }
            }
            Err(TryRecvError::Empty) => DrainStep::Idle,
        }
    }

    pub(super) fn refresh_rows_if_due(&mut self, now: Instant) -> bool {
        if !self.dirty {
            return false;
        }
        let due = self
            .rows_built
            .is_none_or(|built| now.saturating_duration_since(built) >= ROW_REBUILD_INTERVAL);
        if !due {
            return false;
        }
        self.rebuild_rows(now);
        true
    }

    pub(super) fn has_lines_over(&self, keep: usize) -> bool {
        (0..self.store.file_count()).any(|index| {
            u32::try_from(index)
                .ok()
                .and_then(|id| self.store.file(id))
                .is_some_and(|file| file.lines().nth(keep).is_some())
        })
    }

    pub(super) fn trim_lines(&mut self, keep: usize, mark_notice: bool) {
        match self.store.trim_lines(keep) {
            Ok(()) => {
                if mark_notice {
                    self.notice = Some(Notice::Trimmed);
                }
                if keep < LINES_PER_FILE && !self.rows.is_empty() {
                    self.rebuild_rows_now();
                }
            }
            Err(StoreError::OffsetFull) => {
                self.notice = Some(Notice::OffsetsFull);
            }
        }
    }

    fn apply_batch(&mut self, root: u32, batch: FoundBatch) -> bool {
        self.update_progress(root, batch.progress.clone());
        if batch.files.is_empty() {
            return false;
        }
        match self.store.push(root, &batch) {
            Ok(()) => {
                self.dirty = true;
                true
            }
            Err(StoreError::OffsetFull) => {
                self.notice = Some(Notice::OffsetsFull);
                false
            }
        }
    }

    fn update_progress(&mut self, root: u32, progress: FindProgress) {
        if let Some(previous) = self.progress_root
            && previous != root
        {
            add_progress(&mut self.progress_base, &self.root_progress);
        }
        self.progress_root = Some(root);
        self.root_progress = progress;
        self.progress = self.progress_base.clone();
        add_progress(&mut self.progress, &self.root_progress);
    }

    fn record_root_failed(&mut self, root: u32, error: String) {
        let index = root as usize;
        if let Some(location) = self.roots.get(index) {
            self.unreachable.push(UnreachableRoot::WalkFailed {
                location: location.clone(),
                error,
            });
            return;
        }
        let offset = index.saturating_sub(self.roots.len());
        let within = unresolved_within(&self.base, &self.query, offset).unwrap_or_default();
        self.unreachable
            .push(UnreachableRoot::Unresolved { within, error });
    }

    fn finish_run(&mut self, result: Result<FindProgress, String>, now: Instant) {
        self.run = None;
        self.store.finish_index();
        match result {
            Ok(progress) => self.progress = progress,
            Err(error) => self.error = Some(error),
        }
        if self.dirty || self.rows_built.is_none() {
            self.rebuild_rows(now);
        }
    }

    fn rebuild_rows_now(&mut self) {
        self.rows = rows::rows(&self.store, &self.mode);
        self.cursor = if self.rows.is_empty() {
            None
        } else if let Some(previous) = self.cursor {
            let index = rows::index_of(&self.rows, previous)
                .unwrap_or_else(|| rows::nearest(&self.rows, previous));
            self.rows.get(index).copied()
        } else {
            self.rows.first().copied()
        };
        self.scroll.wheel(0, self.rows.len());
        self.dirty = false;
    }
}

fn add_progress(total: &mut FindProgress, delta: &FindProgress) {
    total.scanned = total.scanned.saturating_add(delta.scanned);
    total.hits = total.hits.saturating_add(delta.hits);
    total.matched = total.matched.saturating_add(delta.matched);
    total.skipped = total.skipped.saturating_add(delta.skipped);
    if delta.current.is_some() {
        total.current = delta.current.clone();
    }
}

fn unresolved_within(base: &Location, query: &FindQuery, offset: usize) -> Option<String> {
    query
        .within
        .iter()
        .filter(|within| search_root(base, Some(within)).is_err())
        .nth(offset)
        .cloned()
}
