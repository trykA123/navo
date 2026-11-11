//! The compare screen's update inbox (D23): the app's diff hooks, the queue their sinks fill, and
//! how a burst of updates for one pair is coalesced before the window applies it.

use std::sync::{Arc, Mutex, PoisonError};

use super::{copy, sheet};
use crate::browser::Waker;
use crate::compare_diff::{DiffHooks, DiffSink, DiffUpdate};

/// Updates from open pairs, each tagged with the file screen it was sent to.
pub(super) type Inbox = Arc<Mutex<Vec<(u64, DiffUpdate)>>>;

/// The app's diff hooks and the inbox their sinks fill; one per window.
pub(in crate::window_view) struct DiffSlot {
    pub(super) hooks: Option<Arc<dyn DiffHooks>>,
    pub(super) inbox: Inbox,
    pub(super) wake: Waker,
    pub(super) opened: u64,
    /// The hovered change, the copy in flight and the last copy's note (V27).
    pub(super) copy: copy::CopyState,
    /// The plan sheet of copy all matches (V31).
    pub(super) sheet: sheet::SheetState,
}

impl DiffSlot {
    /// A slot over `hooks` (none until the app wires them) that wakes the window with `wake`.
    pub(in crate::window_view) fn new(hooks: Option<Arc<dyn DiffHooks>>, wake: Waker) -> Self {
        Self {
            hooks,
            inbox: Inbox::default(),
            wake,
            opened: 0,
            copy: copy::CopyState::default(),
            sheet: sheet::SheetState::default(),
        }
    }

    /// A sink that tags each update with `tag`, queues it and wakes the window.
    pub(super) fn sink(&self, tag: u64) -> DiffSink {
        let (inbox, wake) = (Arc::clone(&self.inbox), Arc::clone(&self.wake));
        Arc::new(move |update| {
            coalesce(
                &mut inbox.lock().unwrap_or_else(PoisonError::into_inner),
                tag,
                update,
            );
            wake();
        })
    }
}

/// Queue `update`, coalescing the snapshots the view has not applied yet.
///
/// A newer `Large` snapshot replaces the older one. `Bytes` updates carry only new regions,
/// so an older pending one of the same request, align and run keeps its regions and takes the
/// newer progress.
pub(in crate::window_view) fn coalesce(
    inbox: &mut Vec<(u64, DiffUpdate)>,
    tag: u64,
    update: DiffUpdate,
) {
    match update {
        DiffUpdate::Large { request, .. } => {
            inbox.retain(|(sent_to, queued)| {
                *sent_to != tag
                    || !matches!(queued, DiffUpdate::Large { request: queued, .. } if *queued == request)
            });
            inbox.push((tag, update));
        }
        DiffUpdate::Bytes {
            request,
            align,
            run,
            regions,
            progress,
        } => coalesce_bytes(
            inbox,
            tag,
            ByteBatch {
                request,
                align,
                run,
                regions,
                progress,
            },
        ),
        other => inbox.push((tag, other)),
    }
}

/// The fields of one `DiffUpdate::Bytes`, passed to `coalesce_bytes` as one value.
struct ByteBatch {
    request: u64,
    align: crate::compare_diff::HexAlign,
    run: u64,
    regions: Vec<crate::compare_diff::ByteRegionView>,
    progress: Option<crate::compare_diff::ByteProgress>,
}

/// Merge one byte snapshot into the pending snapshot of the same run.
fn coalesce_bytes(inbox: &mut Vec<(u64, DiffUpdate)>, tag: u64, batch: ByteBatch) {
    let ByteBatch {
        request,
        align,
        run,
        regions,
        progress,
    } = batch;
    let pending = inbox
        .iter_mut()
        .rev()
        .find_map(|(sent_to, queued)| match queued {
            DiffUpdate::Bytes {
                request: queued,
                align: older_align,
                run: older_run,
                regions: older,
                progress: older_progress,
            } if *sent_to == tag
                && *queued == request
                && *older_align == align
                && *older_run == run =>
            {
                Some((older, older_progress))
            }
            _ => None,
        });
    if let Some((older, older_progress)) = pending {
        older.extend(regions);
        if progress.is_some() {
            *older_progress = progress;
        }
    } else {
        inbox.push((
            tag,
            DiffUpdate::Bytes {
                request,
                align,
                run,
                regions,
                progress,
            },
        ));
    }
}
