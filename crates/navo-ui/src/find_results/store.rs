//! Arena storage for the find-results window.
//!
//! Folders, files and hit lines keep ranges into one text buffer, so a result set costs one
//! allocation per folder and file instead of one string per hit.

use std::collections::HashMap;
use std::mem::size_of;
use std::ops::Range;

use navo_fs::find::FoundBatch;

/// Most hit lines kept for one file.
pub const LINE_CAP: usize = 200;

#[derive(Clone, Debug, Default)]
pub struct HitStore {
    folders: Vec<FolderRec>,
    files: Vec<FileRec>,
    lines: Vec<LineRec>,
    text: String,
    folder_index: HashMap<(u32, Box<str>), u32>,
    full: bool,
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
    pub fn push(&mut self, root: u32, batch: &FoundBatch) {
        for found in &batch.files {
            let relative = found.relative.to_text();
            let (folder_path, name) = split_relative(&relative);
            let folder = self.intern_folder(root, folder_path);
            let Some(name_range) = self.append_text(name) else {
                self.full = true;
                return;
            };
            let lines = self.store_lines(&found.hits.hits);
            let hits = found.hits.total;
            self.files.push(FileRec {
                folder,
                name: name_range,
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

    /// The file row at `index`.
    ///
    /// # Panics
    ///
    /// Panics when `index` is past `file_count`; callers walk rows built from this store.
    #[must_use]
    pub fn file(&self, index: u32) -> FileView<'_> {
        let file = &self.files[index as usize];
        let folder = &self.folders[file.folder as usize];
        FileView {
            folder: self.slice(&folder.path),
            name: self.slice(&file.name),
            size: file.size,
            modified_ms: file.modified_ms,
            hits: file.hits,
            in_archive: file.in_archive,
            skipped: file.skipped,
            records: &self.lines[range_bounds(&file.lines)],
            text: &self.text,
        }
    }

    /// The folder row at `index`.
    ///
    /// # Panics
    ///
    /// Panics when `index` is past `folder_count`; callers walk rows built from this store.
    #[must_use]
    pub fn folder(&self, index: u32) -> FolderView<'_> {
        let folder = &self.folders[index as usize];
        FolderView {
            root: folder.root,
            path: self.slice(&folder.path),
            files: folder.files,
            hits: folder.hits,
        }
    }

    /// Keeps at most `keep` lines per file and packs the text and line arena again.
    pub fn trim_lines(&mut self, keep: usize) {
        let mut text = String::with_capacity(self.text.len());
        let mut lines = Vec::with_capacity(self.lines.len());
        let mut folders = Vec::with_capacity(self.folders.len());
        for folder in &self.folders {
            let path = append_to(&mut text, self.slice(&folder.path));
            folders.push(FolderRec {
                root: folder.root,
                path,
                files: folder.files,
                hits: folder.hits,
            });
        }
        let mut files = Vec::with_capacity(self.files.len());
        for file in &self.files {
            let name = append_to(&mut text, self.slice(&file.name));
            let start = u32::try_from(lines.len()).unwrap_or(u32::MAX);
            for record in self.lines[range_bounds(&file.lines)].iter().take(keep) {
                let text = append_to(&mut text, self.slice(&record.text));
                lines.push(LineRec {
                    line: record.line,
                    column: record.column,
                    text,
                });
            }
            let end = u32::try_from(lines.len()).unwrap_or(u32::MAX);
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
        self.text = text;
        self.lines = lines;
        self.folders = folders;
        self.files = files;
    }

    /// Drops and shrinks the started-folder index once a run finishes.
    pub fn finish_index(&mut self) {
        self.folder_index.clear();
        self.folder_index.shrink_to_fit();
    }

    /// Stops storing new hit lines; counts keep arriving.
    pub fn stop_lines(&mut self) {
        self.full = true;
    }

    /// Bytes allocated for folders, files, lines, text and the folder index.
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        let records = self.folders.capacity() * size_of::<FolderRec>()
            + self.files.capacity() * size_of::<FileRec>()
            + self.lines.capacity() * size_of::<LineRec>()
            + self.text.capacity();
        let index = self.folder_index.capacity()
            * (size_of::<(u32, Box<str>)>() + size_of::<u32>())
            + self
                .folder_index
                .keys()
                .map(|(_, path)| path.len())
                .sum::<usize>();
        records + index
    }

    fn store_lines(&mut self, hits: &[navo_core::find::LineHit]) -> Range<u32> {
        let start = u32::try_from(self.lines.len()).unwrap_or(u32::MAX);
        if self.full {
            return start..start;
        }
        for hit in hits.iter().take(LINE_CAP) {
            let Some(text) = self.append_text(&hit.text) else {
                self.full = true;
                break;
            };
            self.lines.push(LineRec {
                line: hit.line,
                column: hit.column,
                text,
            });
        }
        let end = u32::try_from(self.lines.len()).unwrap_or(u32::MAX);
        start..end
    }

    fn intern_folder(&mut self, root: u32, path: &str) -> u32 {
        let key = (root, Box::<str>::from(path));
        if let Some(&index) = self.folder_index.get(&key) {
            return index;
        }
        let path = self.append_text(path).unwrap_or(0..0);
        let index = u32::try_from(self.folders.len()).unwrap_or(u32::MAX);
        self.folders.push(FolderRec {
            root,
            path,
            files: 0,
            hits: 0,
        });
        self.folder_index.insert(key, index);
        index
    }

    fn append_text(&mut self, value: &str) -> Option<Range<u32>> {
        let start = u32::try_from(self.text.len()).ok()?;
        let end = u32::try_from(self.text.len().checked_add(value.len())?).ok()?;
        self.text.push_str(value);
        Some(start..end)
    }

    fn slice(&self, range: &Range<u32>) -> &str {
        self.text.get(range_bounds(range)).unwrap_or("")
    }
}

impl FileView<'_> {
    /// The stored hit lines for this file, oldest first.
    pub fn lines(&self) -> impl Iterator<Item = LineView<'_>> {
        self.records.iter().map(|record| LineView {
            line: record.line,
            column: record.column,
            text: self.text.get(range_bounds(&record.text)).unwrap_or(""),
        })
    }
}

fn append_to(text: &mut String, value: &str) -> Range<u32> {
    let start = u32::try_from(text.len()).unwrap_or(u32::MAX);
    text.push_str(value);
    let end = u32::try_from(text.len()).unwrap_or(u32::MAX);
    start..end
}

fn range_bounds(range: &Range<u32>) -> Range<usize> {
    range.start as usize..range.end as usize
}

fn split_relative(relative: &str) -> (&str, &str) {
    match relative.rfind(['/', '\\']) {
        Some(index) => (&relative[..index], &relative[index + 1..]),
        None => ("", relative),
    }
}

fn has_archive_parent(folder: &str) -> bool {
    folder
        .split(['/', '\\'])
        .any(|part| !part.is_empty() && navo_fs::archive::has_archive_name(part))
}
