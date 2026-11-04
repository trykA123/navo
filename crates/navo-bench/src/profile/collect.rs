use std::collections::HashMap;
use std::io::Read;
use std::time::SystemTime;

use super::Counts;
use super::histogram::Histogram;
use super::kinds::{self, Kind};
use super::layouts as lay;
use crate::source::{EntryKind, ReadOnlySource, Record, RecordId};

/// Beyond this many files, link counts are read for every eighth file only.
const LINK_CHECK_FULL: u64 = 200_000;
const LINK_SAMPLE_EVERY: u64 = 8;
const EXTENSION_CAP: usize = 50_000;

pub(super) struct Dir {
    pub(super) parent: Option<RecordId>,
    pub(super) files: u64,
    pub(super) bytes: u64,
    pub(super) git: bool,
}

#[derive(Default)]
pub(super) struct ExtensionStat {
    pub(super) files: u64,
    pub(super) bytes: u64,
    /// At most two distinct folders are kept; the fold needs no more.
    pub(super) folders: Vec<Option<RecordId>>,
}

pub(super) struct Collector {
    pub(super) counts: Counts,
    pub(super) max_depth: u64,
    pub(super) size: Histogram,
    pub(super) name_length: Histogram,
    pub(super) path_length: Histogram,
    pub(super) depth: Histogram,
    pub(super) age: Histogram,
    pub(super) dirs: HashMap<Option<RecordId>, Dir>,
    pub(super) extensions: HashMap<String, ExtensionStat>,
    pub(super) overflow: ExtensionStat,
    sniff_left: u32,
    link_seen: u64,
    now: SystemTime,
}

impl Collector {
    pub(super) fn new(now: SystemTime) -> Self {
        let root = Dir {
            parent: None,
            files: 0,
            bytes: 0,
            git: false,
        };
        Self {
            counts: Counts::default(),
            max_depth: 0,
            size: Histogram::new(&lay::SIZE),
            name_length: Histogram::new(&lay::NAME_LENGTH),
            path_length: Histogram::new(&lay::PATH_LENGTH),
            depth: Histogram::new(&lay::DEPTH),
            age: Histogram::new(&lay::AGE),
            dirs: HashMap::from([(None, root)]),
            extensions: HashMap::new(),
            overflow: ExtensionStat::default(),
            sniff_left: kinds::SNIFF_FILES,
            link_seen: 0,
            now,
        }
    }

    pub(super) fn add(&mut self, source: &ReadOnlySource, record: &Record) {
        self.max_depth = self.max_depth.max(to_u64(record.depth));
        self.add_shape(record);
        self.add_flags(record);
        match record.kind {
            EntryKind::Directory => self.add_directory(record),
            EntryKind::SymbolicLink => self.counts.symlinks += 1,
            EntryKind::Junction => self.counts.junctions += 1,
            EntryKind::ReparsePoint => self.counts.reparse_points += 1,
            EntryKind::File => self.add_file(source, record),
            EntryKind::Other => {}
        }
    }

    fn add_shape(&mut self, record: &Record) {
        let path_chars = to_u64(record.path_chars);
        self.name_length
            .add(&lay::NAME_LENGTH, to_u64(record.name_chars), 0);
        self.path_length.add(&lay::PATH_LENGTH, path_chars, 0);
        if path_chars > lay::LONG_PATH {
            self.counts.paths_over_260 += 1;
        }
        if record.git_marker
            && let Some(dir) = self.dirs.get_mut(&record.parent)
            && !dir.git
        {
            dir.git = true;
            self.counts.git_repos += 1;
        }
    }

    fn add_flags(&mut self, record: &Record) {
        let (hidden, system, read_only) = flags(record);
        self.counts.hidden += u64::from(hidden);
        self.counts.system += u64::from(system);
        self.counts.read_only += u64::from(read_only);
    }

    fn add_directory(&mut self, record: &Record) {
        self.counts.folders += 1;
        self.dirs.insert(
            Some(record.id.clone()),
            Dir {
                parent: record.parent.clone(),
                files: 0,
                bytes: 0,
                git: false,
            },
        );
    }

    fn add_file(&mut self, source: &ReadOnlySource, record: &Record) {
        self.counts.files += 1;
        self.size.add(&lay::SIZE, record.size, record.size);
        self.depth
            .add(&lay::DEPTH, to_u64(record.depth), record.size);
        self.counts.over_100_mb += u64::from(record.size > 100 * lay::MIB);
        self.counts.over_1_gb += u64::from(record.size > lay::GIB);
        if let Some(dir) = self.dirs.get_mut(&record.parent) {
            dir.files += 1;
            dir.bytes = dir.bytes.saturating_add(record.size);
        }
        self.add_age(record);
        self.add_extension(record);
        self.add_links(source, record);
        let by_extension = kinds::from_extension(&record.extension.to_ascii_lowercase());
        let sniffed = self.sniff(source, record);
        self.count_kind(by_extension.or(sniffed));
    }

    fn add_age(&mut self, record: &Record) {
        let age = record.mtime.map(|time| {
            self.now
                .duration_since(time)
                .map_or(0, |span| span.as_secs())
        });
        match age {
            Some(seconds) => self.age.add(&lay::AGE, seconds, record.size),
            None => self.counts.time_unknown += 1,
        }
    }

    fn add_extension(&mut self, record: &Record) {
        let key = record.extension.to_ascii_lowercase();
        let full = self.extensions.len() >= EXTENSION_CAP && !self.extensions.contains_key(&key);
        let stat = if full {
            &mut self.overflow
        } else {
            self.extensions.entry(key).or_default()
        };
        stat.files += 1;
        stat.bytes = stat.bytes.saturating_add(record.size);
        if stat.folders.len() < 2 && !stat.folders.contains(&record.parent) {
            stat.folders.push(record.parent.clone());
        }
    }

    fn add_links(&mut self, source: &ReadOnlySource, record: &Record) {
        if let Some(links) = self.link_count(source, record) {
            self.counts.hard_link_checked += 1;
            self.counts.hard_linked += u64::from(links > 1);
        }
    }

    fn link_count(&mut self, source: &ReadOnlySource, record: &Record) -> Option<u64> {
        if record.hard_links.is_some() {
            return record.hard_links;
        }
        self.link_seen += 1;
        let due =
            self.link_seen <= LINK_CHECK_FULL || self.link_seen.is_multiple_of(LINK_SAMPLE_EVERY);
        if !due {
            return None;
        }
        source.open_read(&record.id).ok()?.hard_links().ok()
    }

    fn sniff(&mut self, source: &ReadOnlySource, record: &Record) -> Option<Kind> {
        if self.sniff_left == 0 || record.size == 0 {
            return None;
        }
        self.sniff_left -= 1;
        let mut head = Vec::with_capacity(kinds::SNIFF_BYTES);
        let file = source.open_read(&record.id).ok()?;
        let limit = u64::try_from(kinds::SNIFF_BYTES).unwrap_or(0);
        file.take(limit).read_to_end(&mut head).ok()?;
        self.counts.sniffed += 1;
        if kinds::is_text(&head) {
            self.counts.text += 1;
        } else {
            self.counts.binary += 1;
        }
        kinds::from_magic(&head)
    }

    fn count_kind(&mut self, kind: Option<Kind>) {
        let slot = match kind {
            Some(Kind::Archive) => &mut self.counts.archives,
            Some(Kind::Image) => &mut self.counts.images,
            Some(Kind::Pdf) => &mut self.counts.pdf,
            Some(Kind::Elf) => &mut self.counts.elf,
            Some(Kind::Map) => &mut self.counts.map,
            Some(Kind::A2l) => &mut self.counts.a2l,
            Some(Kind::Dbc) => &mut self.counts.dbc,
            None => return,
        };
        *slot += 1;
    }
}

fn to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(windows)]
fn flags(record: &Record) -> (bool, bool, bool) {
    let attributes = record.attributes;
    (
        attributes & 0x2 != 0,
        attributes & 0x4 != 0,
        attributes & 0x1 != 0,
    )
}

#[cfg(not(windows))]
fn flags(record: &Record) -> (bool, bool, bool) {
    (record.dot_name, false, record.attributes & 0o222 == 0)
}
