//! Pure skip rules: when a kept status may stand in for a new one, and the deadlines
//! that follow a fresh badge.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};

use navo_fs::DriveClass;
use navo_git::status::{Fingerprint, RepoStatus};

use super::WATCH_CAP;

/// A badge is never reused after this age.
pub(crate) const MAX_PROOF_AGE: Duration = Duration::from_secs(5 * 60);
/// A confirm runs after the watch debouncer's longest delay, plus margin.
pub(crate) const CONFIRM_WAIT: Duration = Duration::from_secs(1);
/// Most bytes of kept statuses held at once.
pub(crate) const PROOF_BYTES: usize = 4 * 1024 * 1024;

/// One armed watch at one epoch, with the raw changes it has counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Mark {
    pub epoch: u64,
    pub changes: u64,
}

/// The mark, fingerprint and upstream a status was checked against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Proof {
    pub mark: Mark,
    pub fingerprint: Fingerprint,
    pub upstream: Option<String>,
    pub checked_at: Instant,
}

/// A status the scanner may show again, with the proof that it is current.
#[derive(Debug, Clone)]
pub(crate) struct Kept {
    pub proof: Proof,
    pub status: Arc<RepoStatus>,
}

impl Kept {
    fn bytes(&self) -> usize {
        512 + 128 * self.status.dirs.len()
    }
}

/// The first reason a kept status cannot be reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Why {
    Forced,
    Truncated,
    NotFixed,
    NoProof,
    Expired,
    NoCoverage,
    Watched,
}

/// What the confirm step decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    Reuse,
    Unproved,
    Fingerprint,
}

/// What the scanner knows about one root when it decides.
pub(crate) struct Facts {
    pub forced: bool,
    pub truncated: bool,
    pub class: DriveClass,
    pub kept: Option<Kept>,
    pub mark: Option<Mark>,
    pub now: Instant,
}

/// Checks the cheap facts in their decision order.
pub(crate) fn precheck(facts: &Facts) -> Result<(), Why> {
    if facts.forced {
        return Err(Why::Forced);
    }
    if facts.truncated {
        return Err(Why::Truncated);
    }
    if !matches!(facts.class, DriveClass::Ssd | DriveClass::Hdd) {
        return Err(Why::NotFixed);
    }
    let Some(kept) = &facts.kept else {
        return Err(Why::NoProof);
    };
    match facts.now.checked_duration_since(kept.proof.checked_at) {
        Some(age) if age < MAX_PROOF_AGE => {}
        _ => return Err(Why::Expired),
    }
    let Some(mark) = facts.mark else {
        return Err(Why::NoCoverage);
    };
    if mark != kept.proof.mark {
        return Err(Why::Watched);
    }
    Ok(())
}

/// Rechecks the facts, then compares the repository fingerprint.
pub(crate) fn confirm(facts: &Facts, fingerprint: impl FnOnce() -> Option<Fingerprint>) -> Verdict {
    if precheck(facts).is_err() {
        return Verdict::Unproved;
    }
    let Some(kept) = &facts.kept else {
        return Verdict::Unproved;
    };
    match fingerprint() {
        Some(current) if current == kept.proof.fingerprint => Verdict::Reuse,
        Some(_) => Verdict::Fingerprint,
        None => Verdict::Unproved,
    }
}

/// A proof, only when both fingerprints agree and the upstream did not change.
#[expect(
    clippy::too_many_arguments,
    reason = "the plan fixes this proof input list"
)]
pub(crate) fn proof(
    pre_mark: Option<Mark>,
    pre: Option<Fingerprint>,
    post: Option<Fingerprint>,
    kept_upstream: Option<&str>,
    status_upstream: Option<&str>,
    checked_at: Instant,
) -> Option<Proof> {
    let mark = pre_mark?;
    let pre = pre?;
    let post = post?;
    if pre != post || kept_upstream != status_upstream {
        return None;
    }
    Some(Proof {
        mark,
        fingerprint: pre,
        upstream: kept_upstream.map(str::to_owned),
        checked_at,
    })
}

/// Kept statuses by root key, least recently used first.
#[derive(Default)]
pub(crate) struct Proofs {
    entries: VecDeque<(String, Kept)>,
    bytes: usize,
}

impl Proofs {
    pub(crate) fn get(&mut self, root: &str) -> Option<Kept> {
        let index = self.entries.iter().position(|(key, _)| key == root)?;
        let entry = self.entries.remove(index)?;
        let kept = entry.1.clone();
        self.entries.push_back(entry);
        Some(kept)
    }

    pub(crate) fn insert(&mut self, root: String, kept: Kept) {
        let bytes = kept.bytes();
        if bytes > PROOF_BYTES {
            return;
        }
        if let Some(index) = self.entries.iter().position(|(key, _)| key == &root) {
            self.drop_entry(index);
        }
        while !self.entries.is_empty()
            && (self.entries.len() >= WATCH_CAP || self.bytes + bytes > PROOF_BYTES)
        {
            self.drop_entry(0);
        }
        if self.bytes + bytes > PROOF_BYTES {
            return;
        }
        self.bytes += bytes;
        self.entries.push_back((root, kept));
    }

    pub(crate) fn remove(&mut self, root: &str) -> Option<Kept> {
        let index = self.entries.iter().position(|(key, _)| key == root)?;
        self.drop_entry(index)
    }

    fn drop_entry(&mut self, index: usize) -> Option<Kept> {
        let (_, kept) = self.entries.remove(index)?;
        self.bytes -= kept.bytes();
        Some(kept)
    }
}

/// Deadlines in time order, each value returned once.
pub(crate) struct Deadlines<T> {
    entries: Vec<(Instant, T)>,
}

impl<T> Default for Deadlines<T> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<T> Deadlines<T> {
    pub(crate) fn insert(&mut self, at: Instant, value: T) {
        let index = self.entries.partition_point(|(when, _)| *when <= at);
        self.entries.insert(index, (at, value));
    }

    pub(crate) fn next(&self) -> Option<Instant> {
        self.entries.first().map(|(at, _)| *at)
    }

    pub(crate) fn pop_due(&mut self, now: Instant) -> Vec<T> {
        let due = self.entries.partition_point(|(at, _)| *at <= now);
        self.entries.drain(..due).map(|(_, value)| value).collect()
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        self.entries.retain(|(_, value)| keep(value));
    }
}

#[cfg(test)]
#[path = "scan_tests/skip.rs"]
mod tests;
