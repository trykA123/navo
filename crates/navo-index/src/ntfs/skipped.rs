//! Resolves journal changes into folders the full build knowingly skipped (unreachable parent
//! chains) and into NTFS metadata folders under `$Extend`, so they never ask for a rebuild that
//! would skip the same folders again.

use std::collections::{HashMap, VecDeque};

use super::{NtfsIndex, Pending, ROOT_REFERENCE, is_root};
use crate::table::{Flags, TableChange, TableError};
use crate::usn::UsnRecord;

const FILE_CREATE: u32 = 0x100;
const FILE_DELETE: u32 = 0x200;
const RENAME_OLD_NAME: u32 = 0x1000;
const RENAME_NEW_NAME: u32 = 0x2000;
/// `$Extend` is MFT record 11 on every NTFS volume, as the root is record 5.
const EXTEND_RECORD: u128 = 11;

/// The parent reference and name one link had before a rename.
struct OldLink {
    parent_id: u128,
    name: String,
}

/// Old links of this batch's paired renames, keyed by file, new parent and new name.
#[derive(Default)]
pub(super) struct RenameSources(HashMap<(u128, u128, String), VecDeque<OldLink>>);

impl RenameSources {
    /// Pairs each new name with the old name before it in this batch, as the journal parser does.
    /// A pair whose old name came in an earlier read has no entry: its old link stays unknown.
    pub(super) fn collect(records: &[UsnRecord]) -> Self {
        let mut open = HashMap::new();
        let mut sources = Self::default();
        for record in records {
            let file_id = record.file_id;
            if record.reason & FILE_DELETE != 0 {
                open.remove(&file_id);
            } else if record.reason & RENAME_NEW_NAME != 0 {
                sources.pair(open.remove(&file_id), record);
            } else if record.reason & RENAME_OLD_NAME != 0 {
                let parent_id = root_alias(record.parent_id);
                let name = record.name.clone();
                open.insert(file_id, OldLink { parent_id, name });
            } else if record.reason & FILE_CREATE != 0 {
                open.remove(&file_id);
            }
        }
        sources
    }

    /// Files the old link (if this batch had one) under the new name's file, parent and name.
    fn pair(&mut self, old: Option<OldLink>, record: &UsnRecord) {
        let Some(old) = old else {
            return;
        };
        let key = (
            record.file_id,
            root_alias(record.parent_id),
            record.name.clone(),
        );
        self.0.entry(key).or_default().push_back(old);
    }

    /// Takes the old link of the next rename of `file_id` to this parent and name.
    fn take(&mut self, file_id: u128, parent_id: u128, name: &str) -> Option<OldLink> {
        let key = (file_id, parent_id, name.to_owned());
        let links = self.0.get_mut(&key)?;
        let old = links.pop_front();
        if links.is_empty() {
            self.0.remove(&key);
        }
        old
    }
}

/// What became of an Add or Rename whose parent the index lacks.
enum Resolution {
    /// The parent is merely unknown, or the change cannot be resolved safely: it waits.
    Blocked,
    /// The build would not index this name either; the index stays as it is.
    Ignored,
    /// The indexed folder moved into a skipped folder; carries the number of entries removed.
    Removed(usize),
}

/// Resolves a change into a folder the build skipped; any other missing parent stays blocked.
/// Fails only when removing a moved folder from the table fails.
fn resolve(
    index: &mut NtfsIndex,
    change: &TableChange,
    missing: u128,
    sources: &mut RenameSources,
) -> Result<Resolution, TableError> {
    match change {
        TableChange::Add {
            file_id,
            parent_id,
            flags,
            ..
        } if into_metadata(index, *parent_id, missing) => {
            resolve_metadata(index, *file_id, *flags, None)
        }
        TableChange::Rename {
            file_id,
            parent_id,
            name,
            flags,
        } if into_metadata(index, *parent_id, missing)
            || (missing == *file_id && is_metadata(index, *parent_id)) =>
        {
            let old = sources.take(*file_id, *parent_id, name);
            resolve_metadata(index, *file_id, *flags, old)
        }
        TableChange::Add {
            file_id,
            parent_id,
            flags,
            ..
        } if into_skipped(index, *parent_id, missing) => Ok(resolve_add(index, *file_id, *flags)),
        TableChange::Rename {
            file_id,
            parent_id,
            name,
            ..
        } if into_skipped(index, *parent_id, missing) => {
            let old = sources.take(*file_id, *parent_id, name);
            resolve_rename(index, *file_id, old)
        }
        _ => Ok(Resolution::Blocked),
    }
}

/// What the index knows of a missing parent reference, named in the rebuild log line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingClass {
    /// `$Extend` or a known NTFS metadata folder below it.
    Metadata,
    /// A folder the last full build left out (its parent chain never reached the root).
    Skipped,
    /// Neither: the index has no record of it.
    Unknown,
}

impl NtfsIndex {
    /// Classifies a missing reference; a folder in both sets counts as metadata, as `resolve` does.
    pub fn missing_class(&self, reference: u128) -> MissingClass {
        if is_metadata(self, reference) {
            MissingClass::Metadata
        } else if self.skipped_folders.contains(&reference) {
            MissingClass::Skipped
        } else {
            MissingClass::Unknown
        }
    }
}

/// Checks whether a reference is `$Extend` itself, in any sequence number.
pub(super) fn is_extend(reference: u128) -> bool {
    reference <= u128::from(u64::MAX) && reference & 0x0000_ffff_ffff_ffff == EXTEND_RECORD
}

/// Checks that the missing reference is the change's parent and `$Extend` or a folder below it.
fn into_metadata(index: &NtfsIndex, parent_id: u128, missing: u128) -> bool {
    parent_id == missing && is_metadata(index, parent_id)
}

/// Checks whether a reference is `$Extend` or a known NTFS metadata folder below it.
fn is_metadata(index: &NtfsIndex, reference: u128) -> bool {
    is_extend(reference) || index.metadata_folders.contains(&reference)
}

/// Resolves a name that appeared in a metadata folder, given the old link this batch paired with
/// a rename. Windows renames a file into `$Extend\$Deleted` when its last link is deleted while
/// open, so the indexed entry leaves with its subtree. A paired old link other than the indexed
/// one is another hard link: a file then stays (ignored) and a folder blocks. An unpaired file
/// (an Add, or an old name in an earlier read) blocks: the link that left is unknown; a folder
/// has one link. Anything the index lacks is ignored. Folders, and the subfolders of a removed
/// one, join the metadata set. Fails only when the table removal fails.
fn resolve_metadata(
    index: &mut NtfsIndex,
    file_id: u128,
    flags: Flags,
    old: Option<OldLink>,
) -> Result<Resolution, TableError> {
    let Some(indexed) = indexed_flags(index, file_id) else {
        if flags.contains(Flags::FOLDER) {
            index.metadata_folders.insert(file_id);
        }
        return Ok(Resolution::Ignored);
    };
    if old.is_none() && !indexed.contains(Flags::FOLDER) {
        return Ok(Resolution::Blocked);
    }
    if let Some(old) = old
        && !holds_link(index, file_id, &old)
    {
        // A non-last link parked by a POSIX delete; the indexed link still exists.
        return Ok(if indexed.contains(Flags::FOLDER) {
            Resolution::Blocked
        } else {
            Resolution::Ignored
        });
    }
    let folders = if indexed.contains(Flags::FOLDER) {
        subtree_folders(index, file_id)
    } else {
        Vec::new()
    };
    let remove = TableChange::Remove { file_id };
    let removed = index.table.apply_in_batch(remove, &mut index.ids)?;
    index.metadata_folders.extend(folders);
    Ok(Resolution::Removed(removed))
}

/// Checks that the missing reference is the change's parent and a folder the build skipped.
fn into_skipped(index: &NtfsIndex, parent_id: u128, missing: u128) -> bool {
    parent_id == missing && index.skipped_folders.contains(&parent_id)
}

/// Resolves an Add into a skipped folder: a file the index lacks is ignored and a folder joins
/// the set. A file the index holds blocks: a hard-link refresh or an unpaired new name upserts it
/// there, and ignoring it would keep a stale path.
fn resolve_add(index: &mut NtfsIndex, file_id: u128, flags: Flags) -> Resolution {
    if indexed_flags(index, file_id).is_some() {
        return Resolution::Blocked;
    }
    if flags.contains(Flags::FOLDER) {
        index.skipped_folders.insert(file_id);
    }
    Resolution::Ignored
}

/// Resolves a Rename into a skipped folder, given the old link this batch paired with it.
/// - No old link blocks.
/// - A link other than the indexed one is ignored for a file and blocks for a folder.
/// - An indexed folder leaves with its subtree, and it and every indexed subfolder join the set:
///   an NTFS folder has one link. A file blocks, because the journal cannot show it has no other
///   link under an indexed folder.
///
/// Fails only when removing the moved folder from the table fails.
fn resolve_rename(
    index: &mut NtfsIndex,
    file_id: u128,
    old: Option<OldLink>,
) -> Result<Resolution, TableError> {
    let Some(old) = old else {
        return Ok(Resolution::Blocked);
    };
    let is_folder =
        indexed_flags(index, file_id).is_some_and(|flags| flags.contains(Flags::FOLDER));
    if !holds_link(index, file_id, &old) {
        // Another hard link of a file moved; the indexed one stays visible.
        return Ok(if is_folder {
            Resolution::Blocked
        } else {
            Resolution::Ignored
        });
    }
    if !is_folder {
        return Ok(Resolution::Blocked);
    }
    let folders = subtree_folders(index, file_id);
    let remove = TableChange::Remove { file_id };
    let removed = index.table.apply_in_batch(remove, &mut index.ids)?;
    index.skipped_folders.extend(folders);
    Ok(Resolution::Removed(removed))
}

/// Lists the file references of `root_file` and of every live indexed folder below it.
/// Walks each folder's parent chain: O(folders x depth), paid only when a folder moves away.
fn subtree_folders(index: &NtfsIndex, root_file: u128) -> Vec<u128> {
    let table = &index.table;
    let Some(&root) = index.ids.get(&root_file) else {
        return vec![root_file];
    };
    let is_folder = |id| {
        table.name(id).is_some()
            && table
                .flags(id)
                .is_some_and(|f: Flags| f.contains(Flags::FOLDER))
    };
    let below_root = |id| {
        std::iter::successors(Some(id), |&id| table.parent(id))
            .take(table.slots() as usize)
            .any(|id| id == root)
    };
    index
        .ids
        .iter()
        .filter(|&(_, id)| is_folder(id) && below_root(id))
        .map(|(file_id, _)| file_id)
        .collect()
}

impl Pending {
    /// Starts a batch that knows the old links of its own paired renames.
    pub(super) fn new(sources: RenameSources) -> Self {
        let mut pending = Self::default();
        pending.sources = sources;
        pending
    }

    /// Applies one change and keeps the skipped set in step: a deleted folder leaves it, a change
    /// into a skipped folder resolves when it can, and a skipped folder that becomes indexed again
    /// is reported unresolved, because its children never were. Returns true when the table took
    /// the change, false when it resolved without it; fails with the table error otherwise.
    pub(super) fn apply_tracked(
        &mut self,
        index: &mut NtfsIndex,
        change: &TableChange,
    ) -> Result<bool, TableError> {
        if let TableChange::Remove { file_id } = change {
            index.skipped_folders.remove(file_id);
            index.metadata_folders.remove(file_id);
        }
        let (applied, removed) = match index.table.apply_in_batch(change.clone(), &mut index.ids) {
            Ok(removed) => {
                self.report_if_rejoined(index, change);
                (true, removed)
            }
            Err(TableError::UnknownFileReference(_)) if self.departed_update(change) => (false, 0),
            Err(TableError::UnknownFileReference(missing)) => {
                let resolution = resolve(index, change, missing, &mut self.sources)?;
                self.note_departed(index, change, &resolution);
                match resolution {
                    Resolution::Blocked => return Err(TableError::UnknownFileReference(missing)),
                    Resolution::Ignored => (false, 0),
                    Resolution::Removed(removed) => (false, removed),
                }
            }
            Err(error) => return Err(error),
        };
        self.removed_subtree |= removed > 1;
        Ok(applied)
    }

    /// Remembers a file whose new name in a metadata folder resolved (Windows parks a file deleted
    /// while open in `$Extend\$Deleted`; an unpaired new name arrives as an Add): the file left
    /// the namespace, so its later size and attribute records in this batch describe nothing the
    /// index shows.
    fn note_departed(&mut self, index: &NtfsIndex, change: &TableChange, resolution: &Resolution) {
        if let TableChange::Add {
            file_id, parent_id, ..
        }
        | TableChange::Rename {
            file_id, parent_id, ..
        } = change
            && !matches!(resolution, Resolution::Blocked)
            && is_metadata(index, *parent_id)
        {
            self.departed.insert(*file_id);
        }
    }

    /// Checks whether a size or attribute record is about a file this batch parked in a metadata
    /// folder; the caller drops it instead of reporting it unresolved.
    fn departed_update(&self, change: &TableChange) -> bool {
        match change {
            TableChange::Size { file_id, .. } | TableChange::Attributes { file_id, .. } => {
                self.departed.contains(file_id)
            }
            _ => false,
        }
    }

    /// Reports a skipped folder that a named change put back in the index, and drops it from the
    /// set; the folder itself is the missing reference, so the service asks for a rebuild.
    fn report_if_rejoined(&mut self, index: &mut NtfsIndex, change: &TableChange) {
        if let TableChange::Add { file_id, .. } | TableChange::Rename { file_id, .. } = change
            && (index.skipped_folders.remove(file_id) | index.metadata_folders.remove(file_id))
        {
            self.report_unresolved(*file_id, [change.clone()]);
        }
    }
}

/// Returns the flags of a live indexed entry, or `None` when the index does not show the file.
fn indexed_flags(index: &NtfsIndex, file_id: u128) -> Option<Flags> {
    let &id = index.ids.get(&file_id)?;
    index.table.name(id)?;
    index.table.flags(id)
}

/// Checks whether the index shows `file_id` under exactly this old parent and name.
fn holds_link(index: &NtfsIndex, file_id: u128, old: &OldLink) -> bool {
    let Some(&id) = index.ids.get(&file_id) else {
        return false;
    };
    let parent = index.ids.get(&old.parent_id).copied();
    index.table.name(id) == Some(old.name.as_str()) && index.table.parent(id) == parent
}

/// Maps every alias of the volume root to its canonical reference.
fn root_alias(reference: u128) -> u128 {
    if is_root(reference) {
        ROOT_REFERENCE
    } else {
        reference
    }
}
