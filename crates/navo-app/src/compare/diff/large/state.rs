//! The running state of one large diff: hunks, steps, progress and deferred acknowledgements.

use std::ops::Range;

use super::acks::Acks;
use navo_compare::model::DiffStep;
use navo_compare::text::diff::Hunk;
use navo_compare::text::large::{Progress, Stage, StepReason};
use navo_ui::compare_diff::{
    LargeHunk, LargeProgress, LargeStage, LargeStep, LargeStepReason, LargeStepView,
};

/// One step for `State::add_step`.
pub(crate) struct Step {
    pub step: DiffStep,
    pub section: Range<u64>,
    pub lines: u64,
    pub reason: StepReason,
}

/// Everything the view has received from the diff so far.
pub(crate) struct State {
    pub(crate) hunks: Vec<LargeHunk>,
    pub(crate) steps: Vec<LargeStepView>,
    pub(crate) progress: Option<LargeProgress>,
    pub(crate) done: bool,
    pub(crate) pending_focus: Option<u64>,
    /// Snapshot numbers and the evictions they hold until the view acknowledges them.
    pub(crate) acks: Acks,
}

impl State {
    /// A clean state before the first event.
    pub(crate) fn new() -> Self {
        Self {
            hunks: Vec::new(),
            steps: Vec::new(),
            progress: None,
            done: false,
            pending_focus: None,
            acks: Acks::default(),
        }
    }

    /// Drop the hunks a section evicted from the view.
    pub(crate) fn drop_hunks(&mut self, left: &Range<u64>, right: &Range<u64>) {
        self.hunks
            .retain(|hunk| !overlaps(&hunk.left_rows, left) && !overlaps(&hunk.right_rows, right));
    }

    /// Add a batch of engine hunks in arrival order.
    pub(crate) fn add_hunks(&mut self, hunks: &[Hunk]) {
        self.hunks.extend(hunks.iter().map(large_hunk));
    }

    /// Clear everything a fresh attempt must rebuild (engine rule 2).
    pub(crate) fn restart(&mut self) {
        self.hunks.clear();
        self.steps.clear();
        self.progress = None;
        self.done = false;
    }

    /// Record a D17 step, ignoring one that is already listed.
    pub(crate) fn add_step(&mut self, input: Step) {
        let step = step_view(input.step, input.section, input.lines, input.reason);
        if !self.steps.contains(&step) {
            self.steps.push(step);
        }
    }
}

/// True when two line ranges share at least one line.
fn overlaps(left: &Range<u64>, right: &Range<u64>) -> bool {
    left.start < right.end && right.start < left.end
}

/// Convert an engine hunk to the UI's line and byte ranges.
fn large_hunk(hunk: &Hunk) -> LargeHunk {
    LargeHunk {
        left_rows: rows(&hunk.left_lines),
        right_rows: rows(&hunk.right_lines),
        left_bytes: hunk.left_bytes.clone(),
        right_bytes: hunk.right_bytes.clone(),
    }
}

/// A 0-based line range widened to the UI's `u64`.
fn rows(range: &Range<usize>) -> Range<u64> {
    let start = u64::try_from(range.start).unwrap_or(u64::MAX);
    let end = u64::try_from(range.end).unwrap_or(u64::MAX);
    start..end
}

/// Convert an engine step and its reason to UI values.
fn step_view(step: DiffStep, section: Range<u64>, lines: u64, reason: StepReason) -> LargeStepView {
    LargeStepView {
        step: match step {
            DiffStep::Whole => LargeStep::Whole,
            DiffStep::Sections => LargeStep::Sections,
            DiffStep::Coarse => LargeStep::Coarse,
            DiffStep::PagedHunks => LargeStep::PagedHunks,
        },
        section,
        lines,
        reason: match reason {
            StepReason::SectionCap => LargeStepReason::SectionCap,
            StepReason::TokenCap => LargeStepReason::TokenCap,
            StepReason::Budget => LargeStepReason::Budget,
            StepReason::Watchdog => LargeStepReason::Watchdog,
            StepReason::Paged => LargeStepReason::Paged,
        },
    }
}

/// Convert the engine's progress stage to the UI's.
pub(super) fn stage_view(stage: Stage) -> LargeStage {
    match stage {
        Stage::Scan => LargeStage::Scan,
        Stage::Sections => LargeStage::Sections,
        Stage::Waiting => LargeStage::Waiting,
    }
}

/// Build a UI progress value from an engine event.
pub(super) fn progress_view(progress: Progress) -> LargeProgress {
    LargeProgress {
        stage: stage_view(progress.stage),
        done: progress.done,
        total: progress.total,
    }
}
