//! Folder listing for the local provider: opens the child iterator and pumps it to the sink
//! in batches, checking the cancel token before every blocking read.

use std::iter::Peekable;
use std::path::Path;

use navo_core::{FsError, FsErrorKind, Location};

use super::{Children, LocalFs, error, open_failure};
use crate::{CancelToken, EntryBatch, FsEntry, ListSink};

impl LocalFs {
    /// Opens the children of `path` (`FindFirstFileExW` on Windows, `read_dir` elsewhere).
    /// Fails with `Cancelled` before the open when `cancel` is set, or with the open error.
    pub(super) fn children<'a>(
        &self,
        path: &Path,
        folder: &'a Location,
        cancel: &CancelToken,
    ) -> Result<Children<'a>, FsError> {
        if cancel.is_cancelled() {
            return Err(error(FsErrorKind::Cancelled, folder));
        }
        #[cfg(windows)]
        if !self.std_listing {
            let children =
                super::win_list::open(path).map_err(|e| open_failure(&e, path, folder))?;
            return Ok(Box::new(
                children.map(move |child| child.map_err(|e| super::io_error(&e, folder))),
            ));
        }
        let children = std::fs::read_dir(path).map_err(|e| open_failure(&e, path, folder))?;
        Ok(Box::new(
            children.filter_map(move |child| super::std_entry(child, folder)),
        ))
    }
}

/// The folder being listed and its cancel token, borrowed from one `list` call.
#[derive(Clone, Copy)]
pub(super) struct Job<'a> {
    pub(super) folder: &'a Location,
    pub(super) cancel: &'a CancelToken,
}

/// Sends `children` to `sink` in batches of `batch_size` entries until the last batch or a break.
/// Fails with `Cancelled` when the job's token is set before a read or after it, or with a read error.
pub(super) fn pump(
    children: Children<'_>,
    batch_size: usize,
    sink: &mut dyn ListSink,
    job: &Job<'_>,
) -> Result<(), FsError> {
    let Job { folder, cancel } = *job;
    let mut children = children.peekable();
    loop {
        if cancel.is_cancelled() {
            return Err(error(FsErrorKind::Cancelled, folder));
        }
        let entries = read_batch(&mut children, batch_size)?;
        let last = children.peek().is_none();
        if cancel.is_cancelled() {
            return Err(error(FsErrorKind::Cancelled, folder));
        }
        if sink.batch(EntryBatch { entries, last }).is_break() || last {
            return Ok(());
        }
    }
}

/// Reads up to `batch_size` entries; each `next` may block (one `FindNextFileW` per buffer).
fn read_batch(
    children: &mut Peekable<Children<'_>>,
    batch_size: usize,
) -> Result<Vec<FsEntry>, FsError> {
    children.by_ref().take(batch_size).collect()
}
