//! The copy request and its refusals: what the UI shows of one change, matched against a fresh
//! diff before anything is written (D29, V26). Pure of writes; the journal does the staging.

use std::collections::BTreeSet;
use std::ops::Range;

use navo_compare::text::{Direction, Whole};
use navo_core::{FileStamp, FsError, Location};
use navo_fs::walk::WalkEntry;
use navo_ui::compare_diff::{PairKind, PairRequest};

use crate::compare::diff::digest::shown_digest;
use crate::compare::diff::load::{LoadInput, Loaded};
use crate::compare::diff::sides::side_of;
use crate::compare::settings::CompareSettings;

#[cfg(test)]
mod tests;

/// What the UI shows of one change: the pair, the byte ranges, the digest and the direction.
/// `lines` copies only those source-side line numbers; `None` copies the whole change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyRequest {
    pub pair: PairRequest,
    pub left_bytes: Range<u64>,
    pub right_bytes: Range<u64>,
    pub digest: [u8; 16],
    pub direction: Direction,
    pub lines: Option<BTreeSet<usize>>,
}

/// Why nothing was copied. The target is untouched in every case but `Unconfirmed`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyRefusal {
    ReadOnly,
    NotCopyable,
    Changed,
    Network,
    InUse,
    Failed(String),
    /// The write may have landed but could not be confirmed (a commit in doubt, or a rollback
    /// left to the next start; NC168). Never undoable now.
    Unconfirmed,
}

impl CopyRefusal {
    /// The refusal as the status line shows it.
    pub fn text(&self) -> String {
        match self {
            Self::Failed(reason) => navo_l10n::t!("compare-copy-failed", reason = reason.as_str()),
            other => navo_l10n::tr(other.id()),
        }
    }

    /// The Fluent id of the refusal.
    pub fn id(&self) -> &'static str {
        match self {
            Self::ReadOnly => "compare-copy-read-only",
            Self::NotCopyable => "compare-copy-not-copyable",
            Self::Changed => "compare-copy-changed",
            Self::Network => "compare-copy-network",
            Self::InUse => "compare-copy-in-use",
            Self::Failed(_) => "compare-copy-failed",
            Self::Unconfirmed => "compare-copy-unconfirmed",
        }
    }
}

/// The change found again in the fresh diff: its index among the hunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown<'a> {
    pub index: usize,
    pub loaded: &'a Loaded,
}

/// The source and target locations of a request; both must be present local files.
/// A missing or non-local target is `ReadOnly` or `NotCopyable` as in Navo.
pub fn ends_of(request: &CopyRequest) -> Result<(Location, Location), CopyRefusal> {
    let pair = &request.pair;
    let (source, target) = match request.direction {
        Direction::LeftToRight => (&pair.left, &pair.right),
        Direction::RightToLeft => (&pair.right, &pair.left),
    };
    let target = target.clone().ok_or(CopyRefusal::NotCopyable)?;
    if !target.is_local() {
        return Err(CopyRefusal::ReadOnly);
    }
    let source = source.clone().ok_or(CopyRefusal::NotCopyable)?;
    if !source.is_local() {
        return Err(CopyRefusal::NotCopyable);
    }
    Ok((source, target))
}

/// Build the load input for a pair from its two local sides, under the compare rules.
/// Fails with `NotCopyable` when a side is missing or cannot be opened.
pub fn load_input(request: &CopyRequest) -> Result<LoadInput, CopyRefusal> {
    let settings = CompareSettings::default();
    let open = |at: &Option<Location>| {
        let at = at.as_ref().ok_or(CopyRefusal::NotCopyable)?;
        side_of(at, &settings).map_err(|_| CopyRefusal::NotCopyable)
    };
    let (options, fallback) = crate::compare::diff::hooks::rules();
    Ok(LoadInput {
        left: Some(open(&request.pair.left)?),
        right: Some(open(&request.pair.right)?),
        options,
        fallback,
    })
}

/// Find the shown change in the fresh diff: equal byte ranges and an equal digest.
/// Fails with `NotCopyable` for a non-text pair, `Changed` when the change moved or differs.
pub fn matched<'a>(request: &CopyRequest, loaded: &'a Loaded) -> Result<Shown<'a>, CopyRefusal> {
    if loaded.kind != PairKind::Text {
        return Err(CopyRefusal::NotCopyable);
    }
    let alignment = loaded.alignment.as_ref().ok_or(CopyRefusal::NotCopyable)?;
    let index = alignment
        .hunks
        .iter()
        .position(|hunk| {
            hunk.left_bytes == request.left_bytes && hunk.right_bytes == request.right_bytes
        })
        .ok_or(CopyRefusal::Changed)?;
    let digest = alignment
        .digests
        .get(&alignment.hunks[index])
        .ok_or(CopyRefusal::NotCopyable)?;
    if shown_digest(&digest) != request.digest {
        return Err(CopyRefusal::Changed);
    }
    Ok(Shown { index, loaded })
}

/// The loaded text of the side a copy writes; `NotCopyable` when that side has none.
pub fn target_whole(direction: Direction, loaded: &Loaded) -> Result<&Whole, CopyRefusal> {
    let side = match direction {
        Direction::LeftToRight => &loaded.right,
        Direction::RightToLeft => &loaded.left,
    };
    side.as_ref().ok_or(CopyRefusal::NotCopyable)
}

/// The stamp the write checks: metadata size and time, and BLAKE3 over the read bytes.
pub fn stamp_of(seen: &WalkEntry, bytes: &[u8]) -> FileStamp {
    FileStamp {
        size: seen.size,
        modified_ms: seen.modified_ms,
        hash: *blake3::hash(bytes).as_bytes(),
    }
}

/// Map a file error to a refusal that carries its message.
pub fn failed(error: FsError) -> CopyRefusal {
    CopyRefusal::Failed(error.message())
}
