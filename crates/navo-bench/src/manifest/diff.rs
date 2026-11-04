use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;

use serde::Serialize;

use super::Manifest;
use super::entry::{Entry, Signature};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Verdict {
    Equal,
    Changed,
    /// A side was not fully walked, or the digests differ with no counted change.
    Incomplete,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Equal => "equal",
            Self::Changed => "changed",
            Self::Incomplete => "incomplete",
        })
    }
}

/// Counts only. An entry that changed in two ways counts in both classes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ManifestDiff {
    pub verdict: Verdict,
    pub added: u64,
    pub removed: u64,
    pub changed_size: u64,
    pub changed_mtime: u64,
    /// Attributes, permissions or hard-link count.
    pub changed_attributes: u64,
    /// File content, or a link's target.
    pub changed_content: u64,
    pub renamed_looking: u64,
    pub before_complete: bool,
    pub after_complete: bool,
}

impl ManifestDiff {
    pub(crate) fn changes(&self) -> u64 {
        self.added
            + self.removed
            + self.changed_size
            + self.changed_mtime
            + self.changed_attributes
            + self.changed_content
            + self.renamed_looking
    }
}

impl fmt::Display for ManifestDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {} added, {} removed, {} renamed, {} size, {} mtime, {} attributes, {} content",
            self.verdict,
            self.added,
            self.removed,
            self.renamed_looking,
            self.changed_size,
            self.changed_mtime,
            self.changed_attributes,
            self.changed_content,
        )
    }
}

#[derive(Default)]
struct Tally {
    added: HashMap<Signature, u64>,
    removed: Vec<Signature>,
    size: u64,
    mtime: u64,
    attributes: u64,
    content: u64,
}

impl Tally {
    fn compare(&mut self, before: &Entry, after: &Entry) {
        if before.kind != after.kind {
            self.removed.push(before.signature());
            self.add(after);
            return;
        }
        self.size += u64::from(before.size != after.size);
        self.mtime += u64::from(before.mtime != after.mtime);
        self.attributes += u64::from(
            before.attributes != after.attributes || before.hard_links != after.hard_links,
        );
        self.content += u64::from(before.payload != after.payload);
    }

    fn add(&mut self, entry: &Entry) {
        *self.added.entry(entry.signature()).or_default() += 1;
    }
}

pub(super) fn compare(before: &Manifest, after: &Manifest) -> ManifestDiff {
    let mut tally = Tally::default();
    merge(&before.entries, &after.entries, &mut tally);
    let mut renamed = 0;
    let mut removed = 0;
    for signature in std::mem::take(&mut tally.removed) {
        match tally.added.get_mut(&signature) {
            Some(left) if *left > 0 => {
                *left -= 1;
                renamed += 1;
            }
            _ => removed += 1,
        }
    }
    let mut diff = ManifestDiff {
        verdict: Verdict::Equal,
        added: tally.added.values().sum(),
        removed,
        changed_size: tally.size,
        changed_mtime: tally.mtime,
        changed_attributes: tally.attributes,
        changed_content: tally.content,
        renamed_looking: renamed,
        before_complete: before.complete,
        after_complete: after.complete,
    };
    diff.verdict = verdict(&diff, before.digest == after.digest);
    diff
}

fn verdict(diff: &ManifestDiff, same_digest: bool) -> Verdict {
    if !diff.before_complete || !diff.after_complete {
        Verdict::Incomplete
    } else if diff.changes() > 0 {
        Verdict::Changed
    } else if same_digest {
        Verdict::Equal
    } else {
        Verdict::Incomplete
    }
}

fn merge(before: &[Entry], after: &[Entry], tally: &mut Tally) {
    let (mut left, mut right) = (0, 0);
    while left < before.len() && right < after.len() {
        match before[left].key.cmp(&after[right].key) {
            Ordering::Less => {
                tally.removed.push(before[left].signature());
                left += 1;
            }
            Ordering::Greater => {
                tally.add(&after[right]);
                right += 1;
            }
            Ordering::Equal => {
                tally.compare(&before[left], &after[right]);
                left += 1;
                right += 1;
            }
        }
    }
    tally
        .removed
        .extend(before[left..].iter().map(Entry::signature));
    for entry in &after[right..] {
        tally.add(entry);
    }
}
