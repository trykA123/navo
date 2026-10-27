use rusty_core::find::{Compiled, FileHits, scan};
use rusty_core::{Location, ProviderPath};

use super::Walk;
use crate::archive::{self, ArchiveEntry, ENTRY_LIMIT, EntryAt, Stamp};
use crate::find::{FindError, FoundFile};
use crate::{Attributes, EntryKind, FsEntry, os_path};

impl Walk<'_> {
    pub(super) fn archive(
        &mut self,
        location: &Location,
        found: &FoundFile,
        compiled: &Compiled,
    ) -> Result<(), FindError> {
        if !has_archive_extension(&found.entry.name.to_text()) {
            return Ok(());
        }
        let path = os_path(location).map_err(FindError::Filesystem)?;
        self.progress.current = Some(location.clone());
        let listing = archive::list_archive(&path, self.cancel.as_atomic_bool());
        self.check_cancelled()?;
        let Ok(listing) = listing else {
            self.progress.skipped += 1;
            return Ok(());
        };
        if listing.truncated {
            self.progress.skipped += 1;
        }
        for (index, entry) in listing.entries.into_iter().enumerate() {
            self.check_cancelled()?;
            if !entry.is_folder {
                let relative = format!(
                    "{}\\{}",
                    found.relative.to_text().replace('/', "\\"),
                    entry.name.replace('/', "\\")
                );
                let found = FoundFile {
                    relative: ProviderPath::from_text(&relative),
                    entry: describe(entry),
                    hits: FileHits::default(),
                };
                self.archive_entry((&path, index), found, compiled)?;
            }
            self.flush_if_due()?;
        }
        Ok(())
    }

    fn archive_entry(
        &mut self,
        source: (&std::path::Path, usize),
        mut found: FoundFile,
        compiled: &Compiled,
    ) -> Result<(), FindError> {
        self.progress.scanned += 1;
        if found.entry.size > ENTRY_LIMIT {
            self.progress.skipped += 1;
            return Ok(());
        }
        if !self.filters.file(None, &found.entry) {
            return Ok(());
        }
        if !self.filters.query.text.is_empty() {
            let at = EntryAt {
                index: source.1,
                limit: ENTRY_LIMIT,
            };
            let bytes = archive::read_archive_entry(source.0, at, self.cancel.as_atomic_bool());
            self.check_cancelled()?;
            let result = bytes
                .map_err(std::io::Error::other)
                .and_then(|bytes| scan(bytes.as_slice(), compiled, self.cancel.as_atomic_bool()));
            match result {
                Ok(hits) => found.hits = hits,
                Err(_) => {
                    self.progress.skipped += 1;
                    return Ok(());
                }
            }
        }
        self.record(found)
    }
}

fn has_archive_extension(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    [
        ".zip",
        ".7z",
        ".tar",
        ".tar.gz",
        ".tgz",
        ".tar.xz",
        ".txz",
        ".tar.zst",
        ".tar.zstd",
        ".tzst",
    ]
    .iter()
    .any(|extension| name.ends_with(extension))
}

fn describe(entry: ArchiveEntry) -> FsEntry {
    let name = entry.name.rsplit(['/', '\\']).next().unwrap_or_default();
    let modified_ms = entry.modified.map(|stamp| match stamp {
        Stamp::UnixMs(value) => value,
        Stamp::Local(value) => {
            let date = rusty_core::find::Date {
                year: value.year,
                month: value.month,
                day: value.day,
            };
            crate::find::date_days(date) * 86_400_000
                + i64::from(value.hour) * 3_600_000
                + i64::from(value.minute) * 60_000
        }
    });
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size: entry.size,
        modified_ms,
        attributes: Attributes {
            hidden: name.starts_with('.'),
            ..Attributes::default()
        },
    }
}
