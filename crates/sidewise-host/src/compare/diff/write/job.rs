//! One blocking copy through the write journal (C3): reloads the pair, matches the shown
//! changes, composes the result, and writes it as one journal job with a backup of the old bytes.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;

use navo_compare::model::CancelToken;
use navo_core::Location;
use navo_fs::walk::stat;
use sidewise_journal::{Expect, JobId, JobKind, Journal, JournalError, WriteError};

use super::compose::{CopyInput, apply_many, open_both};
use super::request::{CopyRefusal, CopyRequest, ends_of, load_input, matched, target_whole};
use crate::compare::diff::load::{Loaded, load_pair};

/// A committed copy: its journal job and the file it wrote, as the UI named it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Committed {
    pub job: JobId,
    pub target: Location,
}

/// Copy `requests`, shown changes of one pair in one direction, as one journal job. Blocks on
/// reads and writes, so it runs on a copy thread. Fails with a refusal and the target as it
/// was, except `Unconfirmed` after a commit in doubt (NC162) or a rollback left to the next
/// start (NC168); a result equal to the target writes nothing and is `NotCopyable` (NC167).
pub fn copy_now(journal: &Journal, requests: &[CopyRequest]) -> Result<Committed, CopyRefusal> {
    copy_between(journal, requests, &mut || {})
}

/// `copy_now` that runs `between` after the target's stat and before the files open; tests
/// swap the target there, as an editor's save by rename would.
pub(crate) fn copy_between(
    journal: &Journal,
    requests: &[CopyRequest],
    between: &mut dyn FnMut(),
) -> Result<Committed, CopyRefusal> {
    let first = requests.first().ok_or(CopyRefusal::NotCopyable)?;
    let one_pair = requests
        .iter()
        .all(|request| request.pair == first.pair && request.direction == first.direction);
    if !one_pair {
        return Err(CopyRefusal::NotCopyable);
    }
    let (source, target) = ends_of(first)?;
    let target_path = navo_fs::os_path(&target).map_err(|_| CopyRefusal::ReadOnly)?;
    let source_path = navo_fs::os_path(&source).map_err(|_| CopyRefusal::NotCopyable)?;
    // NC147: the journal logs targets as UTF-8 text.
    if target_path.to_str().is_none() {
        return Err(CopyRefusal::NotCopyable);
    }
    // Stat before open: a rename-over after the stat then fails the journal's check as Changed.
    let seen = stat(&target_path).map_err(|error| CopyRefusal::Failed(error.to_string()))?;
    between();
    let files = open_both(&target_path, &source_path)?;
    let modified_ms = seen
        .modified_ms
        .ok_or_else(|| CopyRefusal::Failed("the file has no modified time".to_owned()))?;
    let loaded = load_pair(&load_input(first)?, &CancelToken::new());
    let lines = picked_lines(requests)?;
    let indices = matched_hunks(requests, &loaded)?;
    let whole = target_whole(first.direction, &loaded)?;
    if u64::try_from(whole.bytes.len()) != Ok(seen.size) {
        return Err(CopyRefusal::Changed);
    }
    let input = CopyInput {
        loaded: &loaded,
        indices: &indices,
        lines: &lines,
        direction: first.direction,
    };
    let mut composed = Vec::new();
    apply_many(&input, &files, &mut composed)?;
    if composed == whole.bytes {
        return Err(CopyRefusal::NotCopyable);
    }
    let expect = Expect {
        size: Some(seen.size),
        modified_ms: Some(modified_ms),
        hash: None,
    };
    let job = write_job(journal, &target_path, expect, &composed)?;
    Ok(Committed { job, target })
}

/// Write `bytes` over `target` in one Copy job and commit it; the job rolls back on error.
fn write_job(
    journal: &Journal,
    target: &Path,
    expect: Expect,
    bytes: &[u8],
) -> Result<JobId, CopyRefusal> {
    let failed = |error: &dyn std::fmt::Display| CopyRefusal::Failed(error.to_string());
    let mut job = journal
        .begin(JobKind::Copy, "Copy", None)
        .map_err(|error| failed(&error))?;
    let mut fill = |out: &mut dyn Write| out.write_all(bytes).map_err(|error| error.to_string());
    job.overwrite(target, expect, None, &mut fill)
        .map_err(refusal_of)?;
    job.commit().map_err(|error| match error {
        JournalError::InDoubt(_) => CopyRefusal::Unconfirmed,
        other => failed(&other),
    })
}

/// The distinct hunks of every request in the fresh diff, in target order.
fn matched_hunks(requests: &[CopyRequest], loaded: &Loaded) -> Result<Vec<usize>, CopyRefusal> {
    let mut indices = BTreeSet::new();
    for request in requests {
        indices.insert(matched(request, loaded)?.index);
    }
    Ok(indices.into_iter().collect())
}

/// The source-side lines to pick; empty copies every hunk whole. An empty pick, or whole
/// changes mixed with picked ones, is `NotCopyable` (as in Navo).
fn picked_lines(requests: &[CopyRequest]) -> Result<BTreeSet<usize>, CopyRefusal> {
    let mut lines = BTreeSet::new();
    let mut whole = false;
    for request in requests {
        match &request.lines {
            None => whole = true,
            Some(found) if found.is_empty() => return Err(CopyRefusal::NotCopyable),
            Some(found) => lines.extend(found.iter().copied()),
        }
    }
    match (whole, lines.is_empty()) {
        (true, false) => Err(CopyRefusal::NotCopyable),
        _ => Ok(lines),
    }
}

/// The refusal for a failed item write (NC147); the disk-full and other reasons are named,
/// and a rollback left to the next start is `Unconfirmed` (NC168).
fn refusal_of(error: WriteError) -> CopyRefusal {
    match error {
        WriteError::ReadOnly => CopyRefusal::ReadOnly,
        WriteError::Network => CopyRefusal::Network,
        WriteError::Changed => CopyRefusal::Changed,
        WriteError::InUse => CopyRefusal::InUse,
        WriteError::NotRolledBack { .. } => CopyRefusal::Unconfirmed,
        other => CopyRefusal::Failed(other.to_string()),
    }
}
