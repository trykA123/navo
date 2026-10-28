mod archives;

use std::fs::File;
use std::io::{self, Read};
use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use rusty_core::find::{Compiled, FileHits, FindQuery, Skip, scan};
use rusty_core::{Location, ProviderPath};

use super::{Filters, FindError, FindProgress, FoundBatch, FoundFile};
use crate::{CancelToken, EntryBatch, EntryKind, FsEntry, LocalFs, Provider, os_path};

const FILE_LIMIT: u64 = 64 * 1024 * 1024;
const BATCH_FILES: u64 = 200;
const BATCH_TIME: Duration = Duration::from_millis(100);

pub(super) struct Walk<'a> {
    filters: Filters<'a>,
    cancel: &'a CancelToken,
    sink: &'a mut dyn FnMut(FoundBatch) -> ControlFlow<()>,
    progress: FindProgress,
    files: Vec<FoundFile>,
    last: Instant,
    scanned: u64,
}

impl<'a> Walk<'a> {
    pub(super) fn new(
        query: &'a FindQuery,
        cancel: &'a CancelToken,
        sink: &'a mut dyn FnMut(FoundBatch) -> ControlFlow<()>,
    ) -> Self {
        Self {
            filters: Filters::new(query),
            cancel,
            sink,
            progress: FindProgress::default(),
            files: Vec::new(),
            last: Instant::now(),
            scanned: 0,
        }
    }

    pub(super) fn run(
        mut self,
        root: &Location,
        compiled: &Compiled,
    ) -> Result<FindProgress, FindError> {
        let result = self.folders(root, compiled);
        self.flush()?;
        result?;
        Ok(self.progress)
    }

    fn folders(&mut self, root: &Location, compiled: &Compiled) -> Result<(), FindError> {
        let mut pending = vec![(root.clone(), String::new())];
        while let Some(folder) = pending.pop() {
            self.check_cancelled()?;
            self.progress.current = Some(folder.0.clone());
            match self.list(&folder, compiled) {
                Ok(children) => pending.extend(children.into_iter().rev()),
                Err(error @ FindError::Cancelled) => return Err(error),
                Err(error) if folder.0 == *root => return Err(error),
                Err(_) => self.progress.skipped += 1,
            }
            self.flush_if_due()?;
        }
        Ok(())
    }

    fn list(
        &mut self,
        folder: &(Location, String),
        compiled: &Compiled,
    ) -> Result<Vec<(Location, String)>, FindError> {
        let mut children = Vec::new();
        let mut failure = None;
        let cancel = self.cancel;
        let listed = LocalFs::new().list(
            &folder.0,
            &mut |batch: EntryBatch| {
                let result: Result<(), FindError> =
                    batch.entries.into_iter().try_for_each(|entry| {
                        children.extend(self.take(folder, entry, compiled)?);
                        Ok(())
                    });
                match result {
                    Ok(()) => ControlFlow::Continue(()),
                    Err(error) => {
                        failure = Some(error);
                        ControlFlow::Break(())
                    }
                }
            },
            cancel,
        );
        if let Some(error) = failure {
            return Err(error);
        }
        self.check_cancelled()?;
        listed.map_err(FindError::Filesystem)?;
        Ok(children)
    }

    fn take(
        &mut self,
        folder: &(Location, String),
        entry: FsEntry,
        compiled: &Compiled,
    ) -> Result<Option<(Location, String)>, FindError> {
        self.check_cancelled()?;
        let location = folder
            .0
            .join(&entry.name)
            .map_err(|error| FindError::Invalid(error.to_string()))?;
        let name = if folder.1.is_empty() {
            entry.name.to_text()
        } else {
            format!(
                "{}{}{}",
                folder.1,
                std::path::MAIN_SEPARATOR,
                entry.name.to_text()
            )
        };
        if entry.kind == EntryKind::Folder && self.filters.folder(&entry) {
            self.flush_if_due()?;
            return Ok(Some((location, name)));
        }
        if entry.kind == EntryKind::File {
            let found = FoundFile {
                relative: ProviderPath::from_text(&name),
                entry,
                hits: FileHits::default(),
            };
            if self.filters.query.archives {
                self.archive(&location, &found, compiled)?;
            }
            self.file(location, found, compiled)?;
        }
        self.flush_if_due()?;
        Ok(None)
    }

    fn file(
        &mut self,
        location: Location,
        mut found: FoundFile,
        compiled: &Compiled,
    ) -> Result<(), FindError> {
        self.progress.scanned += 1;
        self.progress.current = Some(location.clone());
        if !self.filters.file(Some(&location), &found.entry) {
            return Ok(());
        }
        if found.entry.size > FILE_LIMIT && self.filters.query.size.is_none() {
            self.progress.skipped += 1;
            return Ok(());
        }
        if !self.filters.query.text.is_empty() {
            let Some(hits) = self.scan_file(&location, compiled)? else {
                return Ok(());
            };
            found.hits = hits;
        }
        self.record(found)
    }

    fn record(&mut self, found: FoundFile) -> Result<(), FindError> {
        if found.hits.skipped == Some(Skip::Cancelled) {
            return Err(FindError::Cancelled);
        }
        if found.hits.skipped.is_some() {
            self.progress.skipped += 1;
        } else if found.hits.total > 0 || self.filters.query.text.is_empty() {
            self.progress.hits += u64::from(found.hits.total);
            self.progress.matched += 1;
            self.files.push(found);
        }
        self.check_cancelled()
    }

    fn scan_file(
        &mut self,
        location: &Location,
        compiled: &Compiled,
    ) -> Result<Option<FileHits>, FindError> {
        let path = os_path(location).map_err(FindError::Filesystem)?;
        let result = File::open(path).and_then(|file| {
            let cancel = self.cancel;
            scan(
                ReportingReader { file, walk: self },
                compiled,
                cancel.as_atomic_bool(),
            )
        });
        self.check_cancelled()?;
        match result {
            Ok(hits) => Ok(Some(hits)),
            Err(_) => {
                self.progress.skipped += 1;
                Ok(None)
            }
        }
    }

    fn check_cancelled(&self) -> Result<(), FindError> {
        if self.cancel.is_cancelled() {
            Err(FindError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn flush_if_due(&mut self) -> Result<(), FindError> {
        if self.progress.scanned - self.scanned >= BATCH_FILES || self.last.elapsed() >= BATCH_TIME
        {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), FindError> {
        self.files.sort_by_cached_key(|file| {
            let name = file.relative.to_text();
            let folder = name
                .rsplit_once(std::path::MAIN_SEPARATOR)
                .map_or(String::new(), |(folder, _)| folder.to_owned());
            (folder, name)
        });
        let batch = FoundBatch {
            files: std::mem::take(&mut self.files),
            progress: self.progress.clone(),
        };
        if (self.sink)(batch).is_break() {
            self.cancel.cancel();
            return Err(FindError::Cancelled);
        }
        self.scanned = self.progress.scanned;
        self.last = Instant::now();
        Ok(())
    }
}

struct ReportingReader<'a, 'b> {
    file: File,
    walk: &'a mut Walk<'b>,
}

impl Read for ReportingReader<'_, '_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.walk
            .check_cancelled()
            .and_then(|()| self.walk.flush_if_due())
            .map_err(|_| io::Error::other("search cancelled"))?;
        self.file.read(buffer)
    }
}
