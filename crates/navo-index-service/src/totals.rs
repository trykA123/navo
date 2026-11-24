//! Answers positional folder totals while distinguishing rebuilds from deadline failure; a late
//! deadline keeps the resolved positions, and a folder hidden by the profile scope answers like
//! one that does not exist.

use std::borrow::Cow;
use std::time::{Duration, Instant};

use navo_index::pipe::{FolderTotals, FolderTotalsReply, Reply, folder_paths_within_limits};
use navo_index::table::{FolderLookup, FolderLookupExpired};

use super::answer::{SharedVolume, State, read_before};
use super::profiles::ProfileIds;

/// Answers requested paths within the service's 20 ms read budget.
pub(super) fn answer(volumes: &[SharedVolume], paths: &[String]) -> Reply {
    answer_before(volumes, paths, Instant::now() + Duration::from_millis(20))
}

/// Returns positional totals. When the deadline or a lookup expires, the resolved positions stay
/// and the rest are unavailable, never Busy; a closed profile scope clears every position.
fn answer_before(volumes: &[SharedVolume], paths: &[String], deadline: Instant) -> Reply {
    if !folder_paths_within_limits(paths) {
        return unavailable(0);
    }
    let mut answering = Answering {
        totals: vec![None; paths.len()],
        busy: false,
    };
    let mut expired = false;
    for volume in volumes {
        match answer_volume(volume, paths, &mut answering, deadline) {
            VolumeAnswer::Continue => {}
            VolumeAnswer::Expired => {
                expired = true;
                break;
            }
            VolumeAnswer::Closed => return unavailable(paths.len()),
        }
    }
    let expired = expired || Instant::now() >= deadline;
    Reply {
        totals: Some(FolderTotalsReply {
            totals: answering.totals,
            busy: answering.busy && !expired,
        }),
        ..Reply::default()
    }
}

/// The positions resolved so far and whether a search-only volume left some of them busy.
struct Answering {
    totals: Vec<Option<FolderTotals>>,
    busy: bool,
}

/// How one volume's pass ended: go on, stop at the deadline, or clear every position.
enum VolumeAnswer {
    Continue,
    Expired,
    Closed,
}

/// Fills the positions of `paths` on `volume` before `deadline`.
fn answer_volume(
    volume: &SharedVolume,
    paths: &[String],
    answering: &mut Answering,
    deadline: Instant,
) -> VolumeAnswer {
    let state = (Instant::now() < deadline)
        .then(|| read_before(&volume.shared, deadline))
        .flatten();
    let Some(state) = state else {
        return VolumeAnswer::Expired;
    };
    let mut lookup = None;
    for (path, total) in paths.iter().zip(&mut answering.totals) {
        if !matches_volume(&state, path) {
            continue;
        }
        if is_search_only(&state) {
            answering.busy = true;
            continue;
        }
        if lookup.is_none() {
            lookup = volume_lookup(&state, deadline);
        }
        if lookup
            .as_ref()
            .is_some_and(|lookup| *lookup.profiles == ProfileIds::Closed)
        {
            return VolumeAnswer::Closed;
        }
        match total_for_path(&mut lookup, &state, path, deadline) {
            Ok(Some(found)) => *total = Some(found),
            Ok(None) => {}
            Err(FolderLookupExpired) => return VolumeAnswer::Expired,
        }
    }
    VolumeAnswer::Continue
}

/// Returns an unavailable reply with one empty position per validated path.
fn unavailable(count: usize) -> Reply {
    Reply {
        totals: Some(FolderTotalsReply {
            totals: vec![None; count],
            busy: false,
        }),
        ..Reply::default()
    }
}

/// True while the volume's index is a search-only generation that has no totals yet.
fn is_search_only(state: &State) -> bool {
    state
        .index
        .as_ref()
        .is_some_and(|index| index.is_search_only())
}

/// Checks the volume prefix and separator before accessing its totals storage.
fn matches_volume(state: &State, path: &str) -> bool {
    let root = state.status.root.trim_end_matches(['/', '\\']);
    path.get(..root.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(root))
        && matches!(path.as_bytes().get(root.len()), None | Some(b'/' | b'\\'))
}

/// One volume's folder lookup and the profile ids it hides, made once per request.
struct VolumeLookup<'a> {
    folders: FolderLookup<'a>,
    profiles: Cow<'a, ProfileIds>,
}

/// Builds the volume's lookup and takes its profile ids (cached per table) before `deadline`;
/// `None` without an index. A scope that runs out of time comes back `Closed` and is logged once.
fn volume_lookup(state: &State, deadline: Instant) -> Option<VolumeLookup<'_>> {
    let table = &state.index.as_ref()?.table;
    let profiles = state.search.profile_ids(table, deadline);
    Some(VolumeLookup {
        folders: table.folder_lookup(),
        profiles,
    })
}

/// Looks up a matched full-generation path, failing when its deadline expires; a path at or
/// under a folder the profile scope hides is treated as missing.
fn total_for_path(
    lookup: &mut Option<VolumeLookup<'_>>,
    state: &State,
    path: &str,
    deadline: Instant,
) -> Result<Option<FolderTotals>, FolderLookupExpired> {
    if Instant::now() >= deadline {
        return Err(FolderLookupExpired);
    }
    let complete =
        state.status.progress.is_none() && state.status.errors.is_empty() && state.index.is_some();
    let id = match lookup {
        Some(VolumeLookup { folders, profiles }) => {
            folders.find_folder(path, deadline)?.filter(|&id| {
                state
                    .index
                    .as_ref()
                    .is_none_or(|index| !profiles.hides_path(&index.table, id))
            })
        }
        None => None,
    };
    let found = state
        .index
        .as_ref()
        .and_then(|index| index.table.totals(id?));
    Ok(match found {
        Some(found) => Some(FolderTotals {
            bytes: found.bytes,
            files: found.files,
            folders: found.folders,
            complete,
        }),
        None if !complete => Some(FolderTotals::default()),
        None => None,
    })
}

#[cfg(test)]
#[path = "totals_tests.rs"]
mod tests;
