//! The network-drive limit of a walk: what bounds it and how it is metered.

use std::time::{Duration, Instant};

use navo_core::{NetworkStop, NetworkWalkBudget};

/// The bounds of one stretch of a walk on a network drive.
///
/// The walk stops at whichever bound is hit first, and `continue_walk` starts a fresh stretch.
/// Applied by `navo_core::NetworkWalkBudget`: `>=`, entries before time, a continue restarts both.
/// Entries are counted on both sides together. Bounds are checked between folders, so one
/// huge folder can overshoot them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkCap {
    pub entries: u64,
    pub duration: Duration,
}

/// Which bound stopped the walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopBound {
    Entries,
    Time,
}

/// A walk that waits for `Session::continue_walk`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimitStop {
    pub bound: StopBound,
    /// Entries listed since the walk began, both sides together.
    pub entries: u64,
}

/// Counts the entries and time of the current stretch of a walk.
pub(super) struct Meter {
    budget: Option<NetworkWalkBudget>,
    listed: u64,
    folders: u64,
}

impl Meter {
    pub(super) fn new(cap: Option<NetworkCap>) -> Self {
        let now = Instant::now();
        Self {
            budget: cap.map(|cap| NetworkWalkBudget::with_limits(now, cap.entries, cap.duration)),
            listed: 0,
            folders: 0,
        }
    }

    pub(super) fn add_folder(&mut self, entries: usize) {
        let entries = u64::try_from(entries).unwrap_or(u64::MAX);
        self.listed = self.listed.saturating_add(entries);
        self.folders = self.folders.saturating_add(1);
        if let Some(budget) = &mut self.budget {
            budget.visit(entries, Instant::now());
        }
    }

    /// The bound that is hit. A stretch always lists one folder, so a walk cannot stall.
    pub(super) fn hit(&mut self) -> Option<LimitStop> {
        if self.folders == 0 {
            return None;
        }
        let stop = self.budget.as_mut()?.visit(0, Instant::now())?;
        Some(LimitStop {
            bound: match stop {
                NetworkStop::Entries => StopBound::Entries,
                NetworkStop::Time => StopBound::Time,
            },
            entries: self.listed,
        })
    }

    pub(super) fn restart(&mut self) {
        self.folders = 0;
        if let Some(budget) = &mut self.budget {
            budget.resume(Instant::now());
        }
    }
}
