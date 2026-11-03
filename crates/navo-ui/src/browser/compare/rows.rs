use std::collections::HashMap;

use super::hooks::{CompareRow, CompareTotals, CompareUpdate, RowProgress};

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
pub(super) struct CompareRows {
    generation: u64,
    rows: Vec<CompareRow>,
    by_id: HashMap<u64, usize>,
    children: HashMap<String, Vec<usize>>,
    progress: HashMap<u64, RowProgress>,
    totals: CompareTotals,
    checking: bool,
    /// Entries listed so far while the walk waits at the network limit.
    stopped: Option<u64>,
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
                if !self.admit(generation) {
                    return Applied::Ignored;
                }
                rows.into_iter().for_each(|row| self.upsert(row));
                for step in progress {
                    self.progress.insert(step.id, step);
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
            CompareUpdate::Failed(text) => {
                self.checking = false;
                return Applied::Failed(text);
            }
        }
        Applied::Changed
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

    pub(super) fn by_path(&self, path: &str) -> Option<&CompareRow> {
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

    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }
}
