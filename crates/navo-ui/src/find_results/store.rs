//! Arena storage for the find-results window.
//!
//! Folders, files and hit lines keep ranges into one text buffer, so a result set costs one
//! allocation per folder and file instead of one string per hit.

use std::collections::HashMap;
use std::mem::size_of;
use std::ops::Range;

use navo_core::find::LineHit;
use navo_fs::find::FoundBatch;

/// Most hit lines kept for one file.
pub const LINE_CAP: usize = 200;

/// Control bytes hashbrown keeps past the buckets, matching its SSE2/NEON group width.
const GROUP_WIDTH: usize = 16;

/// The store refused a batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// Storing the batch would pass the store's byte ceiling.
    OffsetFull,
}

/// Why the store stopped keeping data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreStop {
    /// The caller stopped line storage to respect the tab budget.
    Lines,
    /// The text arena passed its u32 offset ceiling; later batches are refused.
    Offsets,
}

#[derive(Clone, Debug)]
pub struct HitStore {
    folders: Vec<FolderRec>,
    files: Vec<FileRec>,
    lines: Vec<LineRec>,
    text: String,
    folder_index: HashMap<(u32, Box<str>), u32>,
    stopped: Option<StoreStop>,
    limit: u32,
}

impl Default for HitStore {
    fn default() -> Self {
        Self {
            folders: Vec::new(),
            files: Vec::new(),
            lines: Vec::new(),
            text: String::new(),
            folder_index: HashMap::new(),
            stopped: None,
            limit: u32::MAX,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FolderRec {
    root: u32,
    path: Range<u32>,
    files: u32,
    hits: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileRec {
    folder: u32,
    name: Range<u32>,
    size: u64,
    modified_ms: Option<i64>,
    hits: u32,
    lines: Range<u32>,
    in_archive: bool,
    skipped: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LineRec {
    line: u64,
    column: u32,
    text: Range<u32>,
}

/// A folder row read out of the store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FolderView<'a> {
    pub root: u32,
    pub path: &'a str,
    pub files: u32,
    pub hits: u32,
}

/// A file row read out of the store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileView<'a> {
    pub folder_id: u32,
    pub folder: &'a str,
    pub name: &'a str,
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub hits: u32,
    pub in_archive: bool,
    pub skipped: bool,
    records: &'a [LineRec],
    text: &'a str,
}

/// One stored hit line read out of the store.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineView<'a> {
    pub line: u64,
    pub column: u32,
    pub text: &'a str,
}

impl HitStore {
    /// Adds one discovery-order batch and counts every hit, stored or not.
    ///
    /// A batch that would pass the text ceiling is refused whole: `Err` means nothing in it was
    /// stored. When lines already stopped, their text is skipped but the counts still arrive.
    pub fn push(&mut self, root: u32, batch: &FoundBatch) -> Result<(), StoreError> {
        if self.stopped == Some(StoreStop::Offsets) {
            return Err(StoreError::OffsetFull);
        }
        if let Err(error) = self.check_batch(root, batch) {
            self.stopped = Some(StoreStop::Offsets);
            return Err(error);
        }
        for found in &batch.files {
            let relative = found.relative.to_text();
            let (folder_path, name) = split_relative(&relative);
            let name = self.append_text(name)?;
            let folder = self.intern_folder(root, folder_path)?;
            let lines = self.store_lines(&found.hits.hits)?;
            let hits = found.hits.total;
            self.files.push(FileRec {
                folder,
                name,
                size: found.entry.size,
                modified_ms: found.entry.modified_ms,
                hits,
                lines,
                in_archive: has_archive_parent(folder_path),
                skipped: found.hits.skipped.is_some(),
            });
            let folder = &mut self.folders[folder as usize];
            folder.files = folder.files.saturating_add(1);
            folder.hits = folder.hits.saturating_add(hits);
        }
        Ok(())
    }

    /// Number of files found across every folder.
    #[must_use]
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Number of folders found across every root.
    #[must_use]
    pub fn folder_count(&self) -> usize {
        self.folders.len()
    }

    /// Total hits counted by the engine, including lines the store did not keep.
    #[must_use]
    pub fn hit_total(&self) -> u32 {
        self.files
            .iter()
            .fold(0_u32, |total, file| total.saturating_add(file.hits))
    }

    /// The file row at `index`, or `None` when it is past `file_count`.
    #[must_use]
    pub fn file(&self, index: u32) -> Option<FileView<'_>> {
        let file = self.files.get(index as usize)?;
        let folder = self.folders.get(file.folder as usize)?;
        Some(FileView {
            folder_id: file.folder,
            folder: self.slice(&folder.path)?,
            name: self.slice(&file.name)?,
            size: file.size,
            modified_ms: file.modified_ms,
            hits: file.hits,
            in_archive: file.in_archive,
            skipped: file.skipped,
            records: self.lines.get(range_bounds(&file.lines))?,
            text: &self.text,
        })
    }

    /// The folder row at `index`, or `None` when it is past `folder_count`.
    #[must_use]
    pub fn folder(&self, index: u32) -> Option<FolderView<'_>> {
        let folder = self.folders.get(index as usize)?;
        Some(FolderView {
            root: folder.root,
            path: self.slice(&folder.path)?,
            files: folder.files,
            hits: folder.hits,
        })
    }

    /// Why the store stopped keeping data, if it did.
    #[must_use]
    pub fn stopped(&self) -> Option<StoreStop> {
        self.stopped
    }

    /// Keeps at most `keep` lines per file and packs the arena again.
    pub fn trim_lines(&mut self, keep: usize) -> Result<(), StoreError> {
        let mut text = String::new();
        let mut lines = Vec::new();
        let mut folders = Vec::with_capacity(self.folders.len());
        for folder in &self.folders {
            let path = self
                .slice(&folder.path)
                .and_then(|path| append_into(&mut text, path))
                .ok_or(StoreError::OffsetFull)?;
            folders.push(FolderRec {
                root: folder.root,
                path,
                files: folder.files,
                hits: folder.hits,
            });
        }
        let mut files = Vec::with_capacity(self.files.len());
        for file in &self.files {
            let name = self
                .slice(&file.name)
                .and_then(|name| append_into(&mut text, name))
                .ok_or(StoreError::OffsetFull)?;
            let start = u32::try_from(lines.len()).map_err(|_| StoreError::OffsetFull)?;
            for record in self.lines[range_bounds(&file.lines)].iter().take(keep) {
                let text = self
                    .slice(&record.text)
                    .and_then(|value| append_into(&mut text, value))
                    .ok_or(StoreError::OffsetFull)?;
                lines.push(LineRec {
                    line: record.line,
                    column: record.column,
                    text,
                });
            }
            let end = u32::try_from(lines.len()).map_err(|_| StoreError::OffsetFull)?;
            files.push(FileRec {
                folder: file.folder,
                name,
                size: file.size,
                modified_ms: file.modified_ms,
                hits: file.hits,
                lines: start..end,
                in_archive: file.in_archive,
                skipped: file.skipped,
            });
        }
        text.shrink_to_fit();
        lines.shrink_to_fit();
        folders.shrink_to_fit();
        files.shrink_to_fit();
        self.text = text;
        self.lines = lines;
        self.folders = folders;
        self.files = files;
        Ok(())
    }

    /// Drops and shrinks the started-folder index once a run finishes.
    pub fn finish_index(&mut self) {
        self.folder_index.clear();
        self.folder_index.shrink_to_fit();
    }

    /// Stops storing new hit lines; counts keep arriving.
    pub fn stop_lines(&mut self) {
        if self.stopped.is_none() {
            self.stopped = Some(StoreStop::Lines);
        }
    }

    /// Bytes allocated for folders, files, lines, text and the folder index.
    ///
    /// The index is a hashbrown map: `capacity` usable slots, a power-of-two bucket array of
    /// about `capacity * 8 / 7` buckets, one control byte per bucket, one `GROUP_WIDTH` control
    /// group, and the key heap behind `Box<str>`.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        let records = self.folders.capacity() * size_of::<FolderRec>()
            + self.files.capacity() * size_of::<FileRec>()
            + self.lines.capacity() * size_of::<LineRec>()
            + self.text.capacity();
        let buckets = index_buckets(self.folder_index.capacity());
        let index = buckets * (size_of::<((u32, Box<str>), u32)>() + 1)
            + if buckets == 0 { 0 } else { GROUP_WIDTH }
            + self
                .folder_index
                .keys()
                .map(|(_, path)| path.len())
                .sum::<usize>();
        records + index
    }

    #[cfg(test)]
    pub(crate) fn with_limit(limit: u32) -> Self {
        Self {
            limit,
            ..Self::default()
        }
    }

    fn check_batch(&self, root: u32, batch: &FoundBatch) -> Result<(), StoreError> {
        let limit = self.limit as usize;
        let mut text = self.text.len();
        let mut lines = self.lines.len();
        let mut files = self.files.len();
        let mut folders = self.folders.len();
        let mut new_folders: Vec<(u32, String)> = Vec::new();
        for found in &batch.files {
            let relative = found.relative.to_text();
            let (folder_path, name) = split_relative(&relative);
            let known = self
                .folder_index
                .contains_key(&(root, Box::<str>::from(folder_path)))
                || new_folders
                    .iter()
                    .any(|(known_root, path)| *known_root == root && path == folder_path);
            if !known {
                text = add_checked(text, folder_path.len(), limit)?;
                folders = folders.checked_add(1).ok_or(StoreError::OffsetFull)?;
                new_folders.push((root, folder_path.to_owned()));
            }
            text = add_checked(text, name.len(), limit)?;
            files = files.checked_add(1).ok_or(StoreError::OffsetFull)?;
            (text, lines) = self.check_lines(&found.hits.hits, text, lines)?;
        }
        u32::try_from(text).map_err(|_| StoreError::OffsetFull)?;
        u32::try_from(lines).map_err(|_| StoreError::OffsetFull)?;
        u32::try_from(files).map_err(|_| StoreError::OffsetFull)?;
        u32::try_from(folders).map_err(|_| StoreError::OffsetFull)?;
        Ok(())
    }

    fn check_lines(
        &self,
        hits: &[LineHit],
        mut text: usize,
        mut lines: usize,
    ) -> Result<(usize, usize), StoreError> {
        if self.stopped.is_some() {
            return Ok((text, lines));
        }
        let limit = self.limit as usize;
        for hit in hits.iter().take(LINE_CAP) {
            text = add_checked(text, hit.text.len(), limit)?;
            lines = lines.checked_add(1).ok_or(StoreError::OffsetFull)?;
        }
        Ok((text, lines))
    }

    fn store_lines(&mut self, hits: &[LineHit]) -> Result<Range<u32>, StoreError> {
        let start = u32::try_from(self.lines.len()).map_err(|_| self.overflow())?;
        if self.stopped.is_some() {
            return Ok(start..start);
        }
        for hit in hits.iter().take(LINE_CAP) {
            let text = self.append_text(&hit.text)?;
            self.lines.push(LineRec {
                line: hit.line,
                column: hit.column,
                text,
            });
        }
        let end = u32::try_from(self.lines.len()).map_err(|_| self.overflow())?;
        Ok(start..end)
    }

    fn intern_folder(&mut self, root: u32, path: &str) -> Result<u32, StoreError> {
        let key = (root, Box::<str>::from(path));
        if let Some(&index) = self.folder_index.get(&key) {
            return Ok(index);
        }
        let path = self.append_text(path)?;
        let index = u32::try_from(self.folders.len()).map_err(|_| self.overflow())?;
        self.folders.push(FolderRec {
            root,
            path,
            files: 0,
            hits: 0,
        });
        self.folder_index.insert(key, index);
        Ok(index)
    }

    fn append_text(&mut self, value: &str) -> Result<Range<u32>, StoreError> {
        let Some(end) = self.text.len().checked_add(value.len()) else {
            return Err(self.overflow());
        };
        if end > self.limit as usize {
            return Err(self.overflow());
        }
        let start = u32::try_from(self.text.len()).map_err(|_| self.overflow())?;
        let end = u32::try_from(end).map_err(|_| self.overflow())?;
        self.text.push_str(value);
        Ok(start..end)
    }

    fn overflow(&mut self) -> StoreError {
        self.stopped = Some(StoreStop::Offsets);
        StoreError::OffsetFull
    }

    fn slice(&self, range: &Range<u32>) -> Option<&str> {
        self.text.get(range_bounds(range))
    }
}

impl FileView<'_> {
    /// The stored hit lines for this file, oldest first.
    pub fn lines(&self) -> impl Iterator<Item = LineView<'_>> {
        self.records.iter().filter_map(move |record| {
            Some(LineView {
                line: record.line,
                column: record.column,
                text: self.text.get(range_bounds(&record.text))?,
            })
        })
    }
}

/// Slots hashbrown allocates for a map whose usable capacity is `capacity`.
fn index_buckets(capacity: usize) -> usize {
    if capacity == 0 {
        return 0;
    }
    (capacity.saturating_mul(8) / 7).max(4).next_power_of_two()
}

fn add_checked(total: usize, add: usize, limit: usize) -> Result<usize, StoreError> {
    total
        .checked_add(add)
        .filter(|end| *end <= limit)
        .ok_or(StoreError::OffsetFull)
}

fn append_into(text: &mut String, value: &str) -> Option<Range<u32>> {
    let start = u32::try_from(text.len()).ok()?;
    text.push_str(value);
    let end = u32::try_from(text.len()).ok()?;
    Some(start..end)
}

fn range_bounds(range: &Range<u32>) -> Range<usize> {
    range.start as usize..range.end as usize
}

fn split_relative(relative: &str) -> (&str, &str) {
    match relative.rfind(is_separator) {
        Some(index) => (&relative[..index], &relative[index + 1..]),
        None => ("", relative),
    }
}

/// A name separator: Windows accepts both kinds, Unix only its own slash.
fn is_separator(character: char) -> bool {
    character == '/' || (cfg!(windows) && character == '\\')
}

fn has_archive_parent(folder: &str) -> bool {
    folder
        .split(is_separator)
        .any(|part| !part.is_empty() && navo_fs::archive::has_archive_name(part))
}
