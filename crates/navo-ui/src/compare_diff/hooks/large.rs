//! View types for large text and byte diffs, so the UI stays free of `navo-compare`.

use std::ops::Range;

/// One hunk of a large text diff: line and byte ranges on both sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeHunk {
    pub left_rows: Range<u64>,
    pub right_rows: Range<u64>,
    pub left_bytes: Range<u64>,
    pub right_bytes: Range<u64>,
}

/// One D17 detail step a large diff took.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LargeStep {
    Whole,
    Sections,
    Coarse,
    PagedHunks,
}

/// Why a section took a cheaper step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LargeStepReason {
    SectionCap,
    TokenCap,
    Budget,
    Watchdog,
    Paged,
}

/// A step with the section it applied to and the lines it covers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeStepView {
    pub step: LargeStep,
    pub section: Range<u64>,
    pub lines: u64,
    pub reason: LargeStepReason,
}

/// What a large diff is doing now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LargeStage {
    Scan,
    Sections,
    Waiting,
}

/// Scan or section progress; `total` is zero while unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LargeProgress {
    pub stage: LargeStage,
    pub done: u64,
    pub total: u64,
}

/// One changed region of a byte diff, by offset on both sides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteRegionView {
    pub left: Range<u64>,
    pub right: Range<u64>,
    pub changed: u64,
}

/// Byte-diff progress; `coarse` is set when a region fell back to the offset scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ByteProgress {
    pub done: u64,
    pub total: u64,
    pub coarse: bool,
}
