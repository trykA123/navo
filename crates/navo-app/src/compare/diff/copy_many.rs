//! Plan sheet and one-job copy of many matching changes (D31, V30).
//!
//! The sheet is a dry run: each pair is reloaded and every digest checked. The run checks the
//! target stamps again, stages every accepted file, then writes all files in one `Save` job.

use std::collections::BTreeSet;

use navo_compare::model::CancelToken;
use navo_core::{WriteTarget, write_many_plan};
use navo_fs::os_path;

use super::copy::stage::{CopyInput, Staged, open_both};
use super::copy::{
    CopyContext, CopyRefusal, CopyRequest, ends_of, failed, load_input, matched, stamp_of,
    target_whole,
};
use super::load::{Loaded, load_pair};

pub(super) mod plan;

pub(crate) use plan::Sheet;
use plan::{Accepted, Group};

/// What the compare factory started: the file count and the bytes it writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct JobStarted {
    pub files: usize,
    pub bytes: u64,
}

/// A fresh look at an accepted file, ready to stage.
struct Prepared {
    loaded: Loaded,
    hunks: Vec<usize>,
    lines: BTreeSet<usize>,
}

/// Stage every accepted file, then run one `Save` plan through the compare factory.
///
/// Any refusal drops the staged files, so nothing is written.
pub(crate) fn run_sheet(
    sheet: &Sheet,
    requests: &[CopyRequest],
    ctx: &CopyContext<'_>,
) -> Result<JobStarted, CopyRefusal> {
    if requests.len() != sheet.request_count {
        return Err(CopyRefusal::Failed(
            "the plan sheet and its requests differ".to_owned(),
        ));
    }
    let mut staged: Vec<Staged> = Vec::new();
    let mut writes = Vec::new();
    for group in &sheet.groups {
        let Some(accepted) = group.accepted.as_ref() else {
            continue;
        };
        let (write, file) = stage_file(group, requests, accepted, ctx)?;
        writes.push(write);
        staged.push(file);
    }
    if writes.is_empty() {
        return Err(sheet
            .groups
            .iter()
            .find_map(|group| group.why.clone())
            .unwrap_or(CopyRefusal::NotCopyable));
    }
    let plan = write_many_plan(&writes).map_err(|error| CopyRefusal::Failed(error.to_string()))?;
    let started = JobStarted {
        files: writes.len(),
        bytes: plan.totals.bytes,
    };
    super::copy::job::run(ctx, &plan)?;
    Ok(started)
}

/// Reload an accepted file and check its target stamp before any staging.
fn prepare(
    group: &Group,
    requests: &[CopyRequest],
    accepted: &Accepted,
    ctx: &CopyContext<'_>,
) -> Result<Prepared, CopyRefusal> {
    let first = first_request(group, requests)?;
    let seen = ctx.ops.metadata(&accepted.target).map_err(failed)?;
    if seen.size != accepted.stamp.size || seen.modified_ms != accepted.stamp.modified_ms {
        return Err(CopyRefusal::Changed);
    }
    let input = load_input(first, ctx)?;
    let loaded = load_pair(&input, &CancelToken::new());
    let hunks = matched_hunks(group, requests, &loaded)?;
    let whole = target_whole(first.direction, &loaded)?;
    if u64::try_from(whole.bytes.len()) != Ok(seen.size)
        || stamp_of(&seen, &whole.bytes) != accepted.stamp
    {
        return Err(CopyRefusal::Changed);
    }
    let lines = picked_lines(group, requests)?;
    Ok(Prepared {
        loaded,
        hunks,
        lines,
    })
}

/// Stage one accepted file and return its write step with the staged owner.
fn stage_file(
    group: &Group,
    requests: &[CopyRequest],
    accepted: &Accepted,
    ctx: &CopyContext<'_>,
) -> Result<(WriteTarget, Staged), CopyRefusal> {
    let prepared = prepare(group, requests, accepted, ctx)?;
    let first = first_request(group, requests)?;
    let (source, _) = ends_of(first)?;
    let target_path = os_path(&accepted.target).map_err(|_| CopyRefusal::ReadOnly)?;
    let source_path = os_path(&source).map_err(|_| CopyRefusal::NotCopyable)?;
    let paged = open_both(&target_path, &source_path)?;
    let mut file = Staged::create(&accepted.target)?;
    let input = CopyInput {
        loaded: &prepared.loaded,
        indices: &prepared.hunks,
        lines: &prepared.lines,
        direction: first.direction,
    };
    let bytes = file.fill_many(&input, &paged)?;
    let write = WriteTarget {
        target: accepted.target.clone(),
        staged: file.location().clone(),
        expect: accepted.stamp,
        staged_len: bytes,
    };
    Ok((write, file))
}

/// The distinct hunks of every request, sorted in target order.
fn matched_hunks(
    group: &Group,
    requests: &[CopyRequest],
    loaded: &Loaded,
) -> Result<Vec<usize>, CopyRefusal> {
    let mut indices = BTreeSet::new();
    for index in &group.requests {
        let request = requests.get(*index).ok_or(CopyRefusal::NotCopyable)?;
        indices.insert(matched(request, loaded)?.index);
    }
    Ok(indices.into_iter().collect())
}

/// The source-side line numbers to pick; empty means every matched hunk is copied whole.
fn picked_lines(group: &Group, requests: &[CopyRequest]) -> Result<BTreeSet<usize>, CopyRefusal> {
    let mut lines = BTreeSet::new();
    let mut picked = false;
    let mut whole = false;
    for index in &group.requests {
        let request = requests.get(*index).ok_or(CopyRefusal::NotCopyable)?;
        match &request.lines {
            None => whole = true,
            Some(found) if found.is_empty() => return Err(CopyRefusal::NotCopyable),
            Some(found) => {
                picked = true;
                lines.extend(found.iter().copied());
            }
        }
    }
    if whole && picked {
        return Err(CopyRefusal::NotCopyable);
    }
    Ok(if whole { BTreeSet::new() } else { lines })
}

/// The first request of a group, or a refusal when the group is empty.
fn first_request<'a>(
    group: &Group,
    requests: &'a [CopyRequest],
) -> Result<&'a CopyRequest, CopyRefusal> {
    group
        .requests
        .first()
        .and_then(|index| requests.get(*index))
        .ok_or(CopyRefusal::NotCopyable)
}
