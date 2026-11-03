use std::ops::Range;

use crate::model::{DiffStep, Reason};

use super::super::diff::Hunk;
use super::super::digest::HunkDigests;
use super::{Covered, Paged, StepReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Scan,
    Sections,
    Waiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub stage: Stage,
    pub done: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub left_lines: u64,
    pub right_lines: u64,
    pub sections: usize,
    pub dropped_anchors: usize,
    pub restarts: u32,
    pub extra_workers: u32,
}

/// After `Restarted`, every earlier `Provisional` and `Hunks` is stale.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffEvent {
    Provisional {
        hunks: Vec<Hunk>,
        digests: HunkDigests,
    },
    Hunks(Covered),
    Progress(Progress),
    Step {
        step: DiffStep,
        section: Range<u64>,
        lines: u64,
        reason: StepReason,
    },
    /// An emitted section drops its line hunks and keeps this count.
    Paged {
        paged: Paged,
        seq: u64,
    },
    Done(Result<Summary, Reason>),
    Restarted,
}
