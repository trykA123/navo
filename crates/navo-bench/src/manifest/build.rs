use crate::source::{EntryKind, ReadOnlySource, Record, RecordId, WalkOutcome, WalkSummary};

use super::entry::{self, Entry, Hash};
use super::hashing::{self, HashFn, Job, Limits};
use super::{Counts, Manifest, ManifestConfig};

/// Collects entries in any order; `finish` makes the result order independent.
pub(super) struct Builder {
    config: ManifestConfig,
    entries: Vec<Entry>,
    candidates: Vec<(usize, RecordId)>,
    counts: Counts,
    broken: bool,
    hash: HashFn,
    grace: std::time::Duration,
}

impl Builder {
    pub(super) fn new(config: ManifestConfig) -> Self {
        Self {
            config,
            entries: Vec::new(),
            candidates: Vec::new(),
            counts: Counts::default(),
            broken: false,
            hash: hashing::production(),
            grace: hashing::GRACE,
        }
    }

    #[cfg(test)]
    pub(super) fn with_hasher(mut self, hash: HashFn, grace: std::time::Duration) -> Self {
        self.hash = hash;
        self.grace = grace;
        self
    }

    pub(super) fn push(&mut self, source: &ReadOnlySource, record: Record) {
        let Ok(relative) = source.relative_path(&record.id) else {
            self.broken = true;
            return;
        };
        let key = entry::path_key(relative);
        let payload = if entry::is_link_kind(record.kind) {
            source
                .link_target(&record.id)
                .ok()
                .map(|target| entry::text_hash(target.as_os_str()))
        } else {
            None
        };
        let item = Entry::from_record(&record, key, payload);
        self.count(&record);
        if record.kind == EntryKind::File && record.size <= self.config.small_file_bytes {
            self.candidates.push((self.entries.len(), record.id));
        }
        self.entries.push(item);
    }

    fn count(&mut self, record: &Record) {
        match record.kind {
            EntryKind::File => {
                self.counts.files += 1;
                self.counts.bytes = self.counts.bytes.saturating_add(record.size);
            }
            EntryKind::Directory => self.counts.dirs += 1,
            EntryKind::Other => self.counts.others += 1,
            _ => self.counts.links += 1,
        }
    }

    /// Hashes in key order so the chosen files do not depend on walk order.
    fn hash_contents(&mut self, source: &ReadOnlySource) {
        let entries = &self.entries;
        self.candidates
            .sort_by_key(|(index, _)| entries[*index].key);
        let jobs = std::mem::take(&mut self.candidates)
            .into_iter()
            .map(|(index, id)| Job {
                index,
                size: self.entries[index].size,
                id,
            })
            .collect();
        let limits = Limits {
            small_file_bytes: self.config.small_file_bytes,
            budget: self.config.content_budget,
            max_time: self.config.walk.max_time,
            grace: self.grace,
        };
        let hashed = hashing::run(source, jobs, limits, self.hash);
        for (index, digest) in hashed.payloads {
            if let Some(item) = self.entries.get_mut(index) {
                item.payload = Some(digest);
            }
        }
        self.counts.hashed_bytes = hashed.spent;
        self.counts.read_failures = hashed.unreadable;
        self.broken |= !hashed.complete;
    }

    pub(super) fn finish(mut self, source: &ReadOnlySource, summary: WalkSummary) -> Manifest {
        self.hash_contents(source);
        self.entries.sort_by_key(|item| item.key);
        let mut digests: Vec<Hash> = self.entries.iter().map(Entry::digest).collect();
        digests.sort_unstable();
        let mut hasher = blake3::Hasher::new();
        hasher.update(&(digests.len() as u64).to_le_bytes());
        for digest in &digests {
            hasher.update(digest);
        }
        Manifest {
            entries: self.entries,
            digest: *hasher.finalize().as_bytes(),
            counts: self.counts,
            complete: !self.broken
                && summary.errors == 0
                && summary.outcome == WalkOutcome::Complete,
        }
    }
}
