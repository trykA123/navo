//! Stage 1 recursive directory walk with network, placeholder, symlink, and size guards.

use std::collections::HashMap;
use std::ops::ControlFlow;

use navo_core::{FsErrorKind, Location};

use super::{DupesError, DupesSource, DuplicateOptions, DuplicateSummary, NetworkScope};
use crate::{CancelToken, EntryBatch, EntryKind, FsEntry, needs_network_bound};

/// A regular, non-placeholder file collected during Stage 1 walk.
#[derive(Debug, Clone)]
pub(super) struct Candidate {
    pub(super) location: Location,
    pub(super) size: u64,
    pub(super) modified_ms: Option<i64>,
}

/// Same-size candidate bucket produced by Stage 1 (guaranteed `candidates.len() >= 2`).
#[derive(Debug)]
pub(super) struct SizeBucket {
    pub(super) size: u64,
    pub(super) candidates: Vec<Candidate>,
}

/// Runs pre-flight guards and Stage 1 walk, returning size buckets with at least two files
/// sorted largest-first for high-value streaming.
pub(super) fn collect_size_buckets(
    source: &dyn DupesSource,
    root: &Location,
    options: &DuplicateOptions,
    cancel: &CancelToken,
    summary: &mut DuplicateSummary,
) -> Result<Vec<SizeBucket>, DupesError> {
    let item_cap = check_preflight(source, root, options, cancel)?;
    let mut state = WalkState {
        min_size: options.min_size,
        item_cap,
        visited: 0,
        summary,
        by_size: HashMap::new(),
    };
    walk_tree(source, root, cancel, &mut state)?;
    Ok(finalize_buckets(state.by_size))
}

fn check_preflight(
    source: &dyn DupesSource,
    root: &Location,
    options: &DuplicateOptions,
    cancel: &CancelToken,
) -> Result<Option<usize>, DupesError> {
    if cancel.is_cancelled() {
        return Err(DupesError::Cancelled);
    }
    if !root.is_local() {
        return Err(DupesError::UnsupportedRoot(root.clone()));
    }
    if matches!(
        options.network_scope,
        NetworkScope::BoundedSubfolder { max_items: 0 }
    ) {
        return Err(DupesError::InvalidNetworkCap);
    }
    if !needs_network_bound(source.drive_class(root)) {
        return Ok(None);
    }
    match options.network_scope {
        NetworkScope::Refuse => Err(DupesError::NetworkRootRefused(root.clone())),
        NetworkScope::BoundedSubfolder { max_items } => Ok(Some(max_items)),
    }
}

struct WalkState<'a> {
    min_size: u64,
    item_cap: Option<usize>,
    visited: usize,
    summary: &'a mut DuplicateSummary,
    by_size: HashMap<u64, Vec<Candidate>>,
}

impl WalkState<'_> {
    fn at_cap(&self) -> bool {
        self.item_cap.is_some_and(|limit| self.visited >= limit)
    }

    fn take_entry(
        &mut self,
        folder: &Location,
        entry: FsEntry,
        children: &mut Vec<Location>,
    ) -> ControlFlow<()> {
        if self.at_cap() {
            self.summary.truncated_by_cap = true;
            return ControlFlow::Break(());
        }
        self.visited = self.visited.saturating_add(1);
        match entry.kind {
            EntryKind::Link => {
                // Never follow symlinks, junctions, or mount-point reparse points.
                self.summary.skipped_links += 1;
            }
            EntryKind::Folder => self.take_folder(folder, &entry, children),
            EntryKind::File => self.take_file(folder, entry),
        }
        ControlFlow::Continue(())
    }

    fn take_folder(&mut self, folder: &Location, entry: &FsEntry, children: &mut Vec<Location>) {
        // NV4a guard: never list or recall an online-only cloud placeholder folder.
        if entry.attributes.cloud_placeholder {
            self.summary.skipped_placeholders += 1;
            return;
        }
        match folder.join(&entry.name) {
            Ok(child) => {
                self.summary.scanned_folders += 1;
                children.push(child);
            }
            Err(_) => self.summary.skipped_errors += 1,
        }
    }

    fn take_file(&mut self, folder: &Location, entry: FsEntry) {
        // NV4a guard: never open, read, or query identity on a cloud placeholder stub.
        if entry.attributes.cloud_placeholder {
            self.summary.skipped_placeholders += 1;
            return;
        }
        self.summary.scanned_files += 1;
        if entry.size < self.min_size {
            self.summary.skipped_small += 1;
            return;
        }
        match folder.join(&entry.name) {
            Ok(location) => {
                self.by_size.entry(entry.size).or_default().push(Candidate {
                    location,
                    size: entry.size,
                    modified_ms: entry.modified_ms,
                });
            }
            Err(_) => self.summary.skipped_errors += 1,
        }
    }
}

fn walk_tree(
    source: &dyn DupesSource,
    root: &Location,
    cancel: &CancelToken,
    state: &mut WalkState<'_>,
) -> Result<(), DupesError> {
    let mut pending = vec![root.clone()];
    while let Some(folder) = pending.pop() {
        if cancel.is_cancelled() {
            return Err(DupesError::Cancelled);
        }
        if state.at_cap() {
            state.summary.truncated_by_cap = true;
            break;
        }
        let mut children = Vec::new();
        let listed = list_folder(source, &folder, cancel, state, &mut children);
        if cancel.is_cancelled() {
            return Err(DupesError::Cancelled);
        }
        match listed {
            Ok(()) => pending.extend(children.into_iter().rev()),
            Err(error) if error.kind == FsErrorKind::Cancelled => {
                return Err(DupesError::Cancelled);
            }
            Err(error) if folder == *root => return Err(DupesError::Filesystem(error)),
            Err(_) => state.summary.skipped_errors += 1,
        }
    }
    Ok(())
}

fn list_folder(
    source: &dyn DupesSource,
    folder: &Location,
    cancel: &CancelToken,
    state: &mut WalkState<'_>,
    children: &mut Vec<Location>,
) -> Result<(), navo_core::FsError> {
    source.list(
        folder,
        &mut |batch: EntryBatch| {
            for entry in batch.entries {
                if cancel.is_cancelled() {
                    return ControlFlow::Break(());
                }
                if state.take_entry(folder, entry, children).is_break() {
                    return ControlFlow::Break(());
                }
            }
            ControlFlow::Continue(())
        },
        cancel,
    )
}

fn finalize_buckets(by_size: HashMap<u64, Vec<Candidate>>) -> Vec<SizeBucket> {
    let mut buckets: Vec<SizeBucket> = by_size
        .into_iter()
        .filter(|(_, entries)| entries.len() >= 2)
        .map(|(size, mut candidates)| {
            candidates.sort_by(|left, right| left.location.as_str().cmp(right.location.as_str()));
            SizeBucket { size, candidates }
        })
        .collect();
    buckets.sort_by_key(|bucket| std::cmp::Reverse(bucket.size));
    buckets
}
