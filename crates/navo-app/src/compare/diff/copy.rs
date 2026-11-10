//! Copy one change between the two files of a pair (D29, V26).
//!
//! The copy re-diffs the pair on fresh instances, matches the shown change by byte ranges and
//! digest, stages the result beside the target and runs one `Save` job through the compare factory.

use std::collections::BTreeSet;
use std::ops::Range;
use std::sync::Arc;

use navo_compare::model::{CancelToken, EntryKind, RelPath, SideEntry};
use navo_compare::text::{Direction, HunkDigest, Options, Whole};
use navo_core::{FileStamp, FsError, Location, write_plan};
use navo_fs::{CancelToken as FsCancel, FsEntry, ProviderOps, os_path};
use navo_ui::browser::LogFactory;
use navo_ui::compare_diff::{PairKind, PairRequest};

use super::load::{LoadInput, Loaded, Side, load_pair};
use crate::compare::local::LocalSide;

pub(in crate::compare::diff) mod job;
pub(in crate::compare::diff) mod stage;

use stage::Staged;

/// What the UI shows of one change: the pair, the byte ranges, the digest and the direction.
/// `lines` copies only those source-side line numbers; `None` copies the whole change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CopyRequest {
    pub pair: PairRequest,
    pub left_bytes: Range<u64>,
    pub right_bytes: Range<u64>,
    pub digest: [u8; 16],
    pub direction: Direction,
    pub lines: Option<BTreeSet<usize>>,
}

/// What a copy needs from the app: the compare rules, the job factory and the file provider.
pub(crate) struct CopyContext<'a> {
    pub options: &'a Options,
    pub fallback: &'static navo_core::encoding::Encoding,
    pub ops: &'a dyn ProviderOps,
    pub factory: &'a LogFactory,
    pub cancel: &'a FsCancel,
}

/// A finished copy: the target and the bytes of the staged result that replaced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Copied {
    pub target: Location,
    pub bytes: u64,
}

/// Why nothing was copied. The target is untouched in every case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CopyRefusal {
    ReadOnly,
    NotCopyable,
    Changed,
    Failed(String),
}

impl CopyRefusal {
    /// The refusal as the status line shows it.
    pub(crate) fn text(&self) -> String {
        match self {
            Self::ReadOnly => navo_l10n::tr("compare-copy-read-only"),
            Self::NotCopyable => navo_l10n::tr("compare-copy-not-copyable"),
            Self::Changed => navo_l10n::tr("compare-copy-changed"),
            Self::Failed(reason) => navo_l10n::t!("compare-copy-failed", reason = reason.as_str()),
        }
    }

    /// The Fluent id of the refusal, for a plan sheet row.
    pub(crate) fn id(&self) -> &'static str {
        match self {
            Self::ReadOnly => "compare-copy-read-only",
            Self::NotCopyable => "compare-copy-not-copyable",
            Self::Changed => "compare-copy-changed",
            Self::Failed(_) => "compare-copy-failed",
        }
    }
}

/// The 128-bit digest the UI shows for a change: BLAKE3 over both byte-range digests.
///
/// It covers both sides, so an edit to either side after the change was shown is caught.
pub(crate) fn shown_digest(digest: &HunkDigest) -> [u8; 16] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&digest.left);
    hasher.update(&digest.right);
    let mut shown = [0u8; 16];
    shown.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    shown
}

/// Copy one change. Blocks on file reads and the job, so call it from an app thread.
///
/// Fails with a refusal when a side is not a local text file, the change is no longer the one
/// shown, the target changed, or staging or the job fails. A refusal leaves no staged file.
pub(crate) fn copy_change(
    request: &CopyRequest,
    ctx: &CopyContext<'_>,
) -> Result<Copied, CopyRefusal> {
    let (source_at, target_at) = ends_of(request)?;
    let target_path = os_path(&target_at).map_err(|_| CopyRefusal::ReadOnly)?;
    let source_path = os_path(&source_at).map_err(|_| CopyRefusal::NotCopyable)?;
    let paged = stage::open_both(&target_path, &source_path)?;
    let seen = ctx.ops.metadata(&target_at).map_err(failed)?;
    let input = load_input(request, ctx)?;
    let loaded = load_pair(&input, &CancelToken::new());
    let shown = matched(request, &loaded)?;
    let target = target_whole(request.direction, &loaded)?;
    if u64::try_from(target.bytes.len()) != Ok(seen.size) {
        return Err(CopyRefusal::Changed);
    }
    let stamp = stamp_of(&seen, &target.bytes);
    let mut staged = Staged::create(&target_at)?;
    let bytes = staged.fill(request, &shown, &paged)?;
    let plan = write_plan(&target_at, staged.location(), stamp, bytes);
    job::run(ctx, &plan)?;
    Ok(Copied {
        target: target_at,
        bytes,
    })
}

/// Build the load input for a pair from its two local sides.
pub(in crate::compare::diff) fn load_input(
    request: &CopyRequest,
    ctx: &CopyContext<'_>,
) -> Result<LoadInput, CopyRefusal> {
    Ok(LoadInput {
        left: Some(side_of(&request.pair.left, ctx)?),
        right: Some(side_of(&request.pair.right, ctx)?),
        options: *ctx.options,
        fallback: ctx.fallback,
    })
}

/// The stamp the write checks: metadata size and time, and BLAKE3 over the read bytes.
pub(in crate::compare::diff) fn stamp_of(seen: &FsEntry, bytes: &[u8]) -> FileStamp {
    FileStamp {
        size: seen.size,
        modified_ms: seen.modified_ms,
        hash: *blake3::hash(bytes).as_bytes(),
    }
}

/// The change found again in the fresh diff, with the rows and facts the apply needs.
pub(super) struct Shown<'a> {
    pub index: usize,
    pub loaded: &'a Loaded,
}

/// The source and target locations of a request; both must be present local files.
pub(in crate::compare::diff) fn ends_of(
    request: &CopyRequest,
) -> Result<(Location, Location), CopyRefusal> {
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

/// Open one local file as a compare side: its folder as the root and its name as the entry.
fn side_of(at: &Option<Location>, ctx: &CopyContext<'_>) -> Result<Side, CopyRefusal> {
    let at = at.as_ref().ok_or(CopyRefusal::NotCopyable)?;
    let (Some(folder), Some(name)) = at.parent_and_name() else {
        return Err(CopyRefusal::NotCopyable);
    };
    let size = ctx.ops.metadata(at).map_err(failed)?.size;
    let side = LocalSide::new(folder).map_err(|_| CopyRefusal::NotCopyable)?;
    let entry = SideEntry::new(RelPath::new(name), EntryKind::File, size, None);
    Ok((Arc::new(side), entry))
}

/// Find the shown change in the fresh diff: equal ranges, a digest, and the same digest value.
pub(in crate::compare::diff) fn matched<'a>(
    request: &CopyRequest,
    loaded: &'a Loaded,
) -> Result<Shown<'a>, CopyRefusal> {
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

/// The loaded text of the side a copy writes.
pub(in crate::compare::diff) fn target_whole(
    direction: Direction,
    loaded: &Loaded,
) -> Result<&Whole, CopyRefusal> {
    let side = match direction {
        Direction::LeftToRight => &loaded.right,
        Direction::RightToLeft => &loaded.left,
    };
    side.as_ref().ok_or(CopyRefusal::NotCopyable)
}

/// Map a file error to a refusal that carries its message.
pub(in crate::compare::diff) fn failed(error: FsError) -> CopyRefusal {
    CopyRefusal::Failed(error.message())
}
