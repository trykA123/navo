use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::EntryKind;

use super::super::stream::ZipRead;
use super::super::{ArchiveEntry, ArchiveKind, ArchiveListing, ZipReader, unix_ms, zipped};
use super::Clock;
use super::cache::State;
use super::diagnostics::UnsafeReason;

const ZIP_IDLE: Duration = Duration::from_secs(20);

pub(super) struct ArchiveIndex {
    pub state: State,
    pub truncated: bool,
    entries: Vec<ArchiveEntry>,
    paths: Vec<(Vec<u8>, usize)>,
    unsafe_entries: Vec<(String, usize)>,
    duplicates: HashSet<Vec<u8>>,
    unsafe_duplicates: HashSet<String>,
    zip: Arc<Mutex<ZipSlot>>,
    zip_archive: bool,
    archive: PathBuf,
    clock: Clock,
}

struct ZipSlot {
    reader: Option<Arc<Mutex<ZipReader>>>,
    last_used: Instant,
    in_use: usize,
}

pub(super) struct Source {
    pub archive: PathBuf,
    pub zip: Option<ZipReader>,
    pub clock: Clock,
}

pub(super) struct Child {
    pub name: Vec<u8>,
    pub kind: EntryKind,
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub duplicate: bool,
    pub unsafe_reason: Option<UnsafeReason>,
    pub encrypted: bool,
    pub special: Option<ArchiveKind>,
}

impl ArchiveIndex {
    pub(super) fn build(listing: ArchiveListing, state: State, source: Source) -> Self {
        let entries = listing.entries;
        let mut safe = HashMap::new();
        let mut unsafe_seen = HashMap::new();
        let mut duplicates = HashSet::new();
        let mut unsafe_duplicates = HashSet::new();
        for (index, entry) in entries.iter().enumerate() {
            match classify(&entry.name) {
                Ok(key) if key.is_empty() => {}
                Ok(key) => record_safe(&mut safe, &mut duplicates, key, index),
                Err(_) => {
                    record_unsafe(&mut unsafe_seen, &mut unsafe_duplicates, &entry.name, index)
                }
            }
        }
        let mut paths: Vec<(Vec<u8>, usize)> = safe.into_iter().collect();
        paths.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        let mut unsafe_entries: Vec<(String, usize)> = unsafe_seen.into_iter().collect();
        unsafe_entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        Self {
            state,
            truncated: listing.truncated,
            entries,
            paths,
            unsafe_entries,
            duplicates,
            unsafe_duplicates,
            zip_archive: source.zip.is_some(),
            zip: Arc::new(Mutex::new(ZipSlot {
                reader: source.zip.map(|zip| Arc::new(Mutex::new(zip))),
                last_used: (source.clock)(),
                in_use: 0,
            })),
            archive: source.archive,
            clock: source.clock,
        }
    }

    pub(super) fn zip(&self) -> Option<ZipRead> {
        if !self.zip_archive {
            return None;
        }
        let now = (self.clock)();
        let mut slot = self.lock_zip();
        self.expire_slot(&mut slot, now);
        if slot.reader.is_none() {
            let file = File::open(&self.archive).ok()?;
            let reader = zipped::open(BufReader::new(file)).ok()?;
            slot.reader = Some(Arc::new(Mutex::new(reader)));
        }
        let reader = Arc::clone(slot.reader.as_ref()?);
        slot.in_use += 1;
        slot.last_used = now;
        drop(slot);
        let zip = Arc::clone(&self.zip);
        let clock = Arc::clone(&self.clock);
        Some(ZipRead::new(
            reader,
            Box::new(move || {
                let mut slot = zip.lock().unwrap_or_else(PoisonError::into_inner);
                slot.in_use = slot.in_use.saturating_sub(1);
                slot.last_used = clock();
            }),
        ))
    }

    pub(super) fn expire_zip(&self, now: Instant) {
        if !self.zip_archive {
            return;
        }
        let mut slot = self.lock_zip();
        self.expire_slot(&mut slot, now);
    }

    fn expire_slot(&self, slot: &mut ZipSlot, now: Instant) {
        if slot.in_use == 0
            && slot.reader.is_some()
            && now.saturating_duration_since(slot.last_used) > ZIP_IDLE
        {
            slot.reader = None;
        }
    }

    fn lock_zip(&self) -> MutexGuard<'_, ZipSlot> {
        self.zip.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) fn lookup(&self, path: &[u8]) -> Option<&ArchiveEntry> {
        self.position(path)
            .map(|at| &self.entries[self.paths[at].1])
    }

    pub(super) fn lookup_with_index(&self, path: &[u8]) -> Option<(usize, &ArchiveEntry)> {
        let at = self.position(path)?;
        let index = self.paths[at].1;
        Some((index, &self.entries[index]))
    }

    pub(super) fn lookup_unsafe(&self, name: &str) -> Option<&ArchiveEntry> {
        self.unsafe_entries
            .binary_search_by(|(known, _)| known.as_str().cmp(name))
            .ok()
            .map(|at| &self.entries[self.unsafe_entries[at].1])
    }

    pub(super) fn has_descendants(&self, path: &[u8]) -> bool {
        let prefix = child_prefix(path);
        self.paths
            .get(self.start(&prefix))
            .is_some_and(|(key, _)| key.starts_with(&prefix))
    }

    pub(super) fn children(&self, inner: &[u8]) -> Vec<Child> {
        let mut children = self.safe_children(inner);
        if inner.is_empty() {
            self.push_unsafe(&mut children);
        }
        children
    }

    fn safe_children(&self, inner: &[u8]) -> Vec<Child> {
        let prefix = child_prefix(inner);
        let start = self.start(&prefix);
        let mut children = Vec::new();
        let mut last: Option<Vec<u8>> = None;
        for (key, index) in &self.paths[start..] {
            if !key.starts_with(&prefix) {
                break;
            }
            let rest = &key[prefix.len()..];
            let Some(name) = rest.split(|byte| *byte == b'/').next() else {
                continue;
            };
            if last.as_deref() == Some(name) {
                continue;
            }
            let exact = rest.len() == name.len();
            let entry = &self.entries[*index];
            let kind = self.child_kind(entry, key, exact);
            children.push(Child {
                name: name.to_vec(),
                kind,
                size: if kind == EntryKind::Folder {
                    0
                } else {
                    entry.size
                },
                modified_ms: if exact { unix_ms(entry.modified) } else { None },
                duplicate: exact && self.duplicates.contains(key),
                unsafe_reason: None,
                encrypted: exact && entry.is_encrypted,
                special: exact.then_some(entry.kind).filter(|kind| kind.is_special()),
            });
            last = Some(name.to_vec());
        }
        children
    }

    fn push_unsafe(&self, children: &mut Vec<Child>) {
        for (name, index) in &self.unsafe_entries {
            let entry = &self.entries[*index];
            children.push(Child {
                name: name.as_bytes().to_vec(),
                kind: if entry.kind.is_folder() {
                    EntryKind::Folder
                } else if matches!(entry.kind, ArchiveKind::Symlink | ArchiveKind::Hardlink) {
                    EntryKind::Link
                } else {
                    EntryKind::File
                },
                size: if entry.kind.is_folder() {
                    0
                } else {
                    entry.size
                },
                modified_ms: unix_ms(entry.modified),
                duplicate: self.unsafe_duplicates.contains(name),
                unsafe_reason: classify(name).err(),
                encrypted: entry.is_encrypted,
                special: Some(entry.kind).filter(|kind| kind.is_special()),
            });
        }
    }

    fn child_kind(&self, entry: &ArchiveEntry, key: &[u8], exact: bool) -> EntryKind {
        if !exact || entry.kind.is_folder() || self.has_descendants(key) {
            EntryKind::Folder
        } else if matches!(entry.kind, ArchiveKind::Symlink | ArchiveKind::Hardlink) {
            EntryKind::Link
        } else {
            EntryKind::File
        }
    }

    fn position(&self, path: &[u8]) -> Option<usize> {
        self.paths
            .binary_search_by(|(key, _)| key.as_slice().cmp(path))
            .ok()
    }

    fn start(&self, prefix: &[u8]) -> usize {
        self.paths
            .partition_point(|(key, _)| key.as_slice() < prefix)
    }
}

pub(super) fn unsafe_reason(name: &str) -> Option<UnsafeReason> {
    classify(name).err()
}

fn classify(name: &str) -> Result<Vec<u8>, UnsafeReason> {
    if name.contains('\0') {
        return Err(UnsafeReason::Nul);
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return Err(UnsafeReason::Absolute);
    }
    let mut parts: Vec<&str> = Vec::new();
    for part in name.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(UnsafeReason::Parent),
            _ if is_drive(part) => return Err(UnsafeReason::Drive),
            _ if invalid_windows_name(part) => return Err(UnsafeReason::WindowsName),
            _ => parts.push(part),
        }
    }
    if parts.is_empty() {
        return Ok(Vec::new());
    }
    Ok(parts.join("/").into_bytes())
}

fn is_drive(part: &str) -> bool {
    let bytes = part.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn invalid_windows_name(part: &str) -> bool {
    if part.ends_with(' ') || part.ends_with('.') {
        return true;
    }
    if part.encode_utf16().count() > 255 {
        return true;
    }
    if part.chars().any(|character| {
        matches!(
            character,
            '<' | '>' | ':' | '"' | '\\' | '|' | '?' | '*' | '\u{0}'..='\u{1f}'
        )
    }) {
        return true;
    }
    reserved_name(part)
}

fn reserved_name(part: &str) -> bool {
    let stem = part.split('.').next().unwrap_or(part).trim_end_matches(' ');
    if ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"]
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
    {
        return true;
    }
    let upper = stem.to_ascii_uppercase();
    if matches!(
        upper.as_str(),
        "COM\u{b9}" | "COM\u{b2}" | "COM\u{b3}" | "LPT\u{b9}" | "LPT\u{b2}" | "LPT\u{b3}"
    ) {
        return true;
    }
    let bytes = upper.as_bytes();
    bytes.len() == 4 && matches!(&upper[..3], "COM" | "LPT") && matches!(bytes[3], b'1'..=b'9')
}

fn record_safe(
    safe: &mut HashMap<Vec<u8>, usize>,
    duplicates: &mut HashSet<Vec<u8>>,
    key: Vec<u8>,
    index: usize,
) {
    if safe.insert(key.clone(), index).is_some() {
        duplicates.insert(key);
    }
}

fn record_unsafe(
    seen: &mut HashMap<String, usize>,
    duplicates: &mut HashSet<String>,
    name: &str,
    index: usize,
) {
    let name = name.to_owned();
    if seen.insert(name.clone(), index).is_some() {
        duplicates.insert(name);
    }
}

fn child_prefix(path: &[u8]) -> Vec<u8> {
    let mut prefix = path.to_vec();
    if !path.is_empty() {
        prefix.push(b'/');
    }
    prefix
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_keep_forward_slashes_and_share_one_format() {
        assert_eq!(classify("a/b/c.txt").unwrap(), b"a/b/c.txt");
        assert_eq!(classify("a//./b/").unwrap(), b"a/b");
        assert_eq!(classify("./").unwrap(), Vec::<u8>::new());
        assert_eq!(classify("a\\b").err(), Some(UnsafeReason::WindowsName));
    }

    #[test]
    fn unsafe_components_are_named() {
        assert_eq!(classify("a/../b").err(), Some(UnsafeReason::Parent));
        assert_eq!(classify("/etc/passwd").err(), Some(UnsafeReason::Absolute));
        assert_eq!(
            classify(r"\server\share").err(),
            Some(UnsafeReason::Absolute)
        );
        assert_eq!(classify("C:/tmp/a").err(), Some(UnsafeReason::Drive));
        assert_eq!(classify("CON.txt").err(), Some(UnsafeReason::WindowsName));
        for reserved in [
            "CONIN$",
            "CONOUT$",
            "COM\u{b9}",
            "COM\u{b2}",
            "COM\u{b3}",
            "LPT\u{b9}",
            "LPT\u{b2}",
            "LPT\u{b3}",
        ] {
            assert_eq!(
                classify(reserved).err(),
                Some(UnsafeReason::WindowsName),
                "{reserved}"
            );
        }
        assert_eq!(classify("CON .txt").err(), Some(UnsafeReason::WindowsName));
        assert_eq!(classify("trail.").err(), Some(UnsafeReason::WindowsName));
        assert_eq!(classify("a<b").err(), Some(UnsafeReason::WindowsName));
        assert_eq!(classify("nul\0byte").err(), Some(UnsafeReason::Nul));
    }
}
