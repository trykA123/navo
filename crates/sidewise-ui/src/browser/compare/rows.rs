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
    /// The oldest content read running at the newest batch; `None` when none runs.
    reading: Option<SlowPath>,
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
                if !self.finish(generation) {
                    return Applied::Ignored;
                }
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
            CompareUpdate::PhaseProgress { .. }
            | CompareUpdate::SlowPath { .. }
            | CompareUpdate::Reading { .. } => {
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

    /// Ends the current generation: no check and no read runs any more; false for another one.
    fn finish(&mut self, generation: u64) -> bool {
        if generation != self.generation {
            return false;
        }
        self.checking = false;
        self.reading = None;
        true
    }

    /// Stores phase progress, a slow path or the running read; false for an older generation or another update.
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
            CompareUpdate::Reading {
                generation,
                path,
                elapsed_ms,
            } => {
                if !self.admit(generation) {
                    return false;
                }
                self.reading = Some(SlowPath { path, elapsed_ms });
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
        // Every batch states the running read; a `Reading` update follows when one runs.
        self.reading = None;
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

    /// The oldest content read running at the newest batch, with its age; `None` when none runs.
    pub fn reading(&self) -> Option<&SlowPath> {
        self.reading.as_ref()
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
mod telemetry_tests;
