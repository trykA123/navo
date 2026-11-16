//! The newest generation's rows: upserts by id and the folder index the views read.

use std::collections::HashMap;

use super::hooks::{ComparePhase, CompareRow, CompareTotals, CompareUpdate, RowProgress, SlowPath};

/// The slow paths a compare keeps; older ones drop, so a UI shows the newest.
const SLOW_PATHS_MAX: usize = 64;

/// What one update did to the received rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Applied {
    /// A stale generation, or nothing new.
    Ignored,
    Changed,
    Failed(String),
}

/// The rows of the newest generation, in arrival order per folder.
#[derive(Debug, Default)]
#[doc(hidden)]
pub struct CompareRows {
    generation: u64,
    rows: Vec<CompareRow>,
    by_id: HashMap<u64, usize>,
    children: HashMap<String, Vec<usize>>,
    progress: HashMap<u64, RowProgress>,
    totals: CompareTotals,
    /// The walk stage the newest phase update reported.
    phase: Option<ComparePhase>,
    /// The slow paths of this generation, oldest first, at most `SLOW_PATHS_MAX`.
    slow_paths: Vec<SlowPath>,
    checking: bool,
    /// Entries listed so far while the walk waits at the network limit.
    stopped: Option<u64>,
    /// Counts the row batches of this generation, so a reader can tell the rows changed.
    batches: u64,
}

impl CompareRows {
    pub(super) fn new() -> Self {
        Self {
            checking: true,
            ..Self::default()
        }
    }

    pub(super) fn apply(&mut self, update: CompareUpdate) -> Applied {
        match update {
            CompareUpdate::Rows {
                generation,
                rows,
                progress,
            } => {
                if !self.apply_rows(generation, rows, progress) {
                    return Applied::Ignored;
                }
            }
            CompareUpdate::Totals { generation, totals } => {
                if !self.admit(generation) {
                    return Applied::Ignored;
                }
                self.totals = totals;
            }
            CompareUpdate::Finished { generation } => {
                if generation != self.generation {
                    return Applied::Ignored;
                }
                self.checking = false;
            }
            CompareUpdate::Stopped {
                generation,
                entries,
                bound: _,
            } => {
                if !self.admit(generation) {
                    return Applied::Ignored;
                }
                self.stopped = Some(entries);
            }
            CompareUpdate::Continued { generation } => {
                if generation != self.generation || self.stopped.take().is_none() {
                    return Applied::Ignored;
                }
            }
            CompareUpdate::PhaseProgress { .. } | CompareUpdate::SlowPath { .. } => {
                if !self.apply_telemetry(update) {
                    return Applied::Ignored;
                }
            }
            CompareUpdate::Failed(text) => {
                self.checking = false;
                return Applied::Failed(text);
            }
        }
        Applied::Changed
    }

    /// Stores phase progress or a slow path; false for an older generation or another update.
    fn apply_telemetry(&mut self, update: CompareUpdate) -> bool {
        match update {
            CompareUpdate::PhaseProgress {
                generation, phase, ..
            } => {
                if !self.admit(generation) {
                    return false;
                }
                self.phase = Some(phase);
            }
            CompareUpdate::SlowPath {
                generation,
                path,
                elapsed_ms,
            } => {
                if !self.admit(generation) {
                    return false;
                }
                if self.slow_paths.len() == SLOW_PATHS_MAX {
                    self.slow_paths.remove(0);
                }
                self.slow_paths.push(SlowPath { path, elapsed_ms });
            }
            _ => return false,
        }
        true
    }

    /// Adds the rows and progress of one batch; false for an older generation.
    fn apply_rows(
        &mut self,
        generation: u64,
        rows: Vec<CompareRow>,
        progress: Vec<RowProgress>,
    ) -> bool {
        if !self.admit(generation) {
            return false;
        }
        self.batches += u64::from(!rows.is_empty());
        rows.into_iter().for_each(|row| self.upsert(row));
        for step in progress {
            self.progress.insert(step.id, step);
        }
        true
    }

    /// False for an older generation; a newer one drops everything received before.
    fn admit(&mut self, generation: u64) -> bool {
        if generation < self.generation {
            return false;
        }
        if generation > self.generation {
            *self = Self {
                generation,
                ..Self::new()
            };
        }
        true
    }

    fn upsert(&mut self, row: CompareRow) {
        if let Some(&at) = self.by_id.get(&row.id) {
            if let Some(slot) = self.rows.get_mut(at) {
                *slot = row;
            }
            return;
        }
        let at = self.rows.len();
        self.by_id.insert(row.id, at);
        self.children
            .entry(row.parent().to_owned())
            .or_default()
            .push(at);
        self.rows.push(row);
    }

    /// The rows listed in `folder`, in arrival order.
    pub(super) fn children<'a>(&'a self, folder: &str) -> impl Iterator<Item = &'a CompareRow> {
        self.children
            .get(folder)
            .into_iter()
            .flatten()
            .filter_map(|&at| self.rows.get(at))
    }

    /// The row with `id`, if it arrived.
    pub(super) fn by_id(&self, id: u64) -> Option<&CompareRow> {
        self.rows.get(*self.by_id.get(&id)?)
    }

    /// Every row under `folder` at any depth, depth first: each row, then the rows inside it.
    /// Rows keep the engine's order within a folder (an explicit stack, no recursion).
    pub(super) fn under(&self, folder: &str) -> Vec<&CompareRow> {
        let mut out = Vec::new();
        let mut stack: Vec<&CompareRow> = self.children(folder).collect();
        stack.reverse();
        while let Some(row) = stack.pop() {
            out.push(row);
            let inside = stack.len();
            stack.extend(self.children(&row.path));
            stack[inside..].reverse();
        }
        out
    }

    #[doc(hidden)]
    pub fn by_path(&self, path: &str) -> Option<&CompareRow> {
        let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
        self.children(parent).find(|row| row.path == path)
    }

    /// The folder the row with `id` is listed in.
    pub(super) fn parent_of(&self, id: u64) -> Option<&str> {
        let row = self.rows.get(*self.by_id.get(&id)?)?;
        Some(row.parent())
    }

    pub(super) fn progress(&self, id: u64) -> Option<RowProgress> {
        self.progress.get(&id).copied()
    }

    pub(super) fn totals(&self) -> CompareTotals {
        self.totals
    }

    pub(super) fn checking(&self) -> bool {
        self.checking
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn stopped(&self) -> Option<u64> {
        self.stopped
    }

    /// The walk stage the newest phase update reported; `None` before the first one.
    pub fn phase(&self) -> Option<ComparePhase> {
        self.phase
    }

    /// The slow files of this generation, oldest first, at most 64.
    pub fn slow_paths(&self) -> &[SlowPath] {
        &self.slow_paths
    }

    /// Row batches received in this generation; it changes whenever a row arrives or changes.
    pub(super) fn batches(&self) -> u64 {
        self.batches
    }

    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }
}

#[cfg(test)]
mod telemetry_tests {
    use super::super::hooks::{ComparePhase, CompareUpdate, SlowPath};
    use super::{Applied, CompareRows};

    /// Opens `generation` with an empty batch, so it is the newest one.
    fn start(rows: &mut CompareRows, generation: u64) {
        let update = CompareUpdate::Rows {
            generation,
            rows: Vec::new(),
            progress: Vec::new(),
        };
        assert_eq!(rows.apply(update), Applied::Changed);
    }

    #[test]
    fn phase_progress_of_the_current_generation_changes_the_rows() {
        let mut rows = CompareRows::new();
        start(&mut rows, 2);
        let update = CompareUpdate::PhaseProgress {
            generation: 2,
            phase: ComparePhase::QuickCheck,
            done: 4,
            total: Some(10),
        };
        assert_eq!(rows.apply(update), Applied::Changed);
        assert_eq!(rows.phase(), Some(ComparePhase::QuickCheck));
    }

    #[test]
    fn a_slow_path_of_the_current_generation_changes_the_rows() {
        let mut rows = CompareRows::new();
        start(&mut rows, 2);
        let update = CompareUpdate::SlowPath {
            generation: 2,
            path: "slow.bin".to_owned(),
            elapsed_ms: 250,
        };
        assert_eq!(rows.apply(update), Applied::Changed);
        assert_eq!(
            rows.slow_paths(),
            [SlowPath {
                path: "slow.bin".to_owned(),
                elapsed_ms: 250,
            }]
            .as_slice()
        );
    }

    #[test]
    fn the_latest_phase_and_slow_paths_read_back_after_updates() {
        let mut rows = CompareRows::new();
        start(&mut rows, 2);
        rows.apply(CompareUpdate::PhaseProgress {
            generation: 2,
            phase: ComparePhase::Listing,
            done: 0,
            total: None,
        });
        rows.apply(CompareUpdate::PhaseProgress {
            generation: 2,
            phase: ComparePhase::ContentCheck,
            done: 3,
            total: Some(9),
        });
        rows.apply(CompareUpdate::SlowPath {
            generation: 2,
            path: "a.bin".to_owned(),
            elapsed_ms: 250,
        });
        rows.apply(CompareUpdate::SlowPath {
            generation: 2,
            path: "b.bin".to_owned(),
            elapsed_ms: 400,
        });
        assert_eq!(
            rows.phase(),
            Some(ComparePhase::ContentCheck),
            "the newest phase wins"
        );
        assert_eq!(
            rows.slow_paths(),
            [
                SlowPath {
                    path: "a.bin".to_owned(),
                    elapsed_ms: 250,
                },
                SlowPath {
                    path: "b.bin".to_owned(),
                    elapsed_ms: 400,
                },
            ]
            .as_slice(),
            "slow paths keep arrival order"
        );
        start(&mut rows, 3);
        assert_eq!(rows.phase(), None, "a new compare starts without a phase");
        assert!(
            rows.slow_paths().is_empty(),
            "a new compare starts without slow paths"
        );
    }

    #[test]
    fn slow_paths_keep_only_the_newest_64() {
        let mut rows = CompareRows::new();
        start(&mut rows, 2);
        for step in 0..65 {
            let update = CompareUpdate::SlowPath {
                generation: 2,
                path: format!("{step}.bin"),
                elapsed_ms: 0,
            };
            assert_eq!(rows.apply(update), Applied::Changed);
        }
        assert_eq!(rows.slow_paths().len(), 64, "the list is bounded");
        assert_eq!(rows.slow_paths()[0].path, "1.bin", "the oldest path drops");
        assert_eq!(
            rows.slow_paths()[63].path,
            "64.bin",
            "the newest path is last"
        );
    }

    #[test]
    fn telemetry_of_an_older_generation_is_ignored() {
        let mut rows = CompareRows::new();
        start(&mut rows, 2);
        let phase = CompareUpdate::PhaseProgress {
            generation: 1,
            phase: ComparePhase::Listing,
            done: 0,
            total: None,
        };
        let slow = CompareUpdate::SlowPath {
            generation: 1,
            path: "old.bin".to_owned(),
            elapsed_ms: 300,
        };
        assert_eq!(rows.apply(phase), Applied::Ignored);
        assert_eq!(rows.apply(slow), Applied::Ignored);
    }
}
