//! The dry-run plan sheet: one row per file, with its matches, bytes and refusal.

use navo_compare::model::CancelToken;
use navo_compare::text::Direction;
use navo_core::{FileStamp, Location};

use super::{first_request, matched_hunks};
use crate::compare::diff::copy::{
    CopyContext, CopyRefusal, CopyRequest, ends_of, failed, load_input, stamp_of, target_whole,
};
use crate::compare::diff::load::{Loaded, load_pair};

/// One file of a plan sheet, as the UI lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SheetFile {
    /// The target path shown to the user.
    pub path: String,
    /// How many distinct changes the file copies.
    pub hunks: usize,
    /// The source bytes those changes carry.
    pub bytes: u64,
    /// The refusal's Fluent id, or `None` when the file will be written.
    pub refusal: Option<&'static str>,
}

/// A dry-run plan: one file per pair and direction, in request order.
#[derive(Debug, Clone)]
pub(crate) struct Sheet {
    pub(crate) files: Vec<SheetFile>,
    pub(super) groups: Vec<Group>,
    pub(super) request_count: usize,
}

/// Requests for one pair and direction, with the stamp the sheet saw.
#[derive(Debug, Clone)]
pub(super) struct Group {
    key: GroupKey,
    pub(super) requests: Vec<usize>,
    pub(super) accepted: Option<Accepted>,
    pub(super) why: Option<CopyRefusal>,
}

/// Requests are grouped when they name the same two locations and direction.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GroupKey {
    left: Option<Location>,
    right: Option<Location>,
    direction: Direction,
}

/// A file the sheet accepted: where it writes and the target stamp seen.
#[derive(Debug, Clone)]
pub(super) struct Accepted {
    pub(super) target: Location,
    pub(super) stamp: FileStamp,
    hunks: usize,
    bytes: u64,
}

/// Plan every file without staging or writing; one entry per pair and direction.
pub(crate) fn plan_sheet(requests: &[CopyRequest], ctx: &CopyContext<'_>) -> Sheet {
    let mut groups = groups_of(requests);
    let files = groups
        .iter_mut()
        .map(|group| plan_file(group, requests, ctx))
        .collect();
    Sheet {
        files,
        groups,
        request_count: requests.len(),
    }
}

/// Group requests by their two locations and direction, keeping request order.
fn groups_of(requests: &[CopyRequest]) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for (index, request) in requests.iter().enumerate() {
        let key = GroupKey {
            left: request.pair.left.clone(),
            right: request.pair.right.clone(),
            direction: request.direction,
        };
        match groups.iter_mut().find(|group| group.key == key) {
            Some(group) => group.requests.push(index),
            None => groups.push(Group {
                key,
                requests: vec![index],
                accepted: None,
                why: None,
            }),
        }
    }
    groups
}

/// Inspect one group and return its sheet row, accepted or refused.
fn plan_file(group: &mut Group, requests: &[CopyRequest], ctx: &CopyContext<'_>) -> SheetFile {
    match inspect(group, requests, ctx) {
        Ok(accepted) => {
            let file = SheetFile {
                path: accepted.target.as_str().to_owned(),
                hunks: accepted.hunks,
                bytes: accepted.bytes,
                refusal: None,
            };
            group.accepted = Some(accepted);
            file
        }
        Err(why) => {
            let path = first_request(group, requests)
                .map_or_else(|_| String::new(), |request| request.pair.path.clone());
            let file = SheetFile {
                path,
                hunks: 0,
                bytes: 0,
                refusal: Some(why.id()),
            };
            group.why = Some(why);
            file
        }
    }
}

/// Reload a pair, match every request by ranges and digest, and keep the target stamp.
fn inspect(
    group: &Group,
    requests: &[CopyRequest],
    ctx: &CopyContext<'_>,
) -> Result<Accepted, CopyRefusal> {
    let first = first_request(group, requests)?;
    let (_, target) = ends_of(first)?;
    let seen = ctx.ops.metadata(&target).map_err(failed)?;
    let input = load_input(first, ctx)?;
    let loaded = load_pair(&input, &CancelToken::new());
    let hunks = matched_hunks(group, requests, &loaded)?;
    let whole = target_whole(first.direction, &loaded)?;
    if u64::try_from(whole.bytes.len()) != Ok(seen.size) {
        return Err(CopyRefusal::Changed);
    }
    Ok(Accepted {
        target,
        stamp: stamp_of(&seen, &whole.bytes),
        hunks: hunks.len(),
        bytes: copied_bytes(&loaded, &hunks, first.direction),
    })
}

/// The source bytes the distinct hunks carry in the copy's direction.
fn copied_bytes(loaded: &Loaded, indices: &[usize], direction: Direction) -> u64 {
    let Some(alignment) = loaded.alignment.as_ref() else {
        return 0;
    };
    indices
        .iter()
        .filter_map(|index| alignment.hunks.get(*index))
        .fold(0, |sum, hunk| {
            let bytes = match direction {
                Direction::LeftToRight => hunk.left_bytes.end.saturating_sub(hunk.left_bytes.start),
                Direction::RightToLeft => {
                    hunk.right_bytes.end.saturating_sub(hunk.right_bytes.start)
                }
            };
            sum.saturating_add(bytes)
        })
}
