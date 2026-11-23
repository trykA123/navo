//! Answers positional folder totals while distinguishing rebuilds from deadline failure; a
//! folder hidden by the profile scope answers like one that does not exist.

use std::time::{Duration, Instant};

use navo_index::pipe::{FolderTotals, FolderTotalsReply, Reply, folder_paths_within_limits};
use navo_index::table::{FolderLookup, FolderLookupExpired};

use super::answer::{SharedVolume, State, read_before};
use super::profiles::ProfileIds;

/// Answers requested paths within the service's 20 ms read budget.
pub(super) fn answer(volumes: &[SharedVolume], paths: &[String]) -> Reply {
    answer_before(volumes, paths, Instant::now() + Duration::from_millis(20))
}

/// Returns positional totals, or clears all results when the deadline or a lookup expires.
fn answer_before(volumes: &[SharedVolume], paths: &[String], deadline: Instant) -> Reply {
    if !folder_paths_within_limits(paths) {
        return unavailable(0);
    }
    let mut totals = vec![None; paths.len()];
    let mut busy = false;
    for volume in volumes {
        if Instant::now() >= deadline {
            return unavailable(paths.len());
        }
        let Some(state) = read_before(&volume.shared, deadline) else {
            return unavailable(paths.len());
        };
        let mut lookup = None;
        for (path, total) in paths.iter().zip(&mut totals) {
            if !matches_volume(&state, path) {
                continue;
            }
            if state
                .index
                .as_ref()
                .is_some_and(|index| index.is_search_only())
            {
                busy = true;
                continue;
            }
            if lookup.is_none() {
                lookup = volume_lookup(&state, deadline);
            }
            if let Some(VolumeLookup {
                profiles: ProfileIds::Closed,
                ..
            }) = lookup
            {
                return unavailable(paths.len());
            }
            match total_for_path(&mut lookup, &state, path, deadline) {
                Ok(Some(found)) => *total = Some(found),
                Ok(None) => {}
                Err(_) => return unavailable(paths.len()),
            }
        }
    }
    if Instant::now() >= deadline {
        return unavailable(paths.len());
    }
    Reply {
        totals: Some(FolderTotalsReply { totals, busy }),
        ..Reply::default()
    }
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
    profiles: ProfileIds,
}

/// Builds the volume's lookup and maps its profile scope before `deadline`; `None` without an
/// index. A scope that runs out of time comes back `Closed` and is logged once per service start.
fn volume_lookup(state: &State, deadline: Instant) -> Option<VolumeLookup<'_>> {
    let mut folders = state.index.as_ref()?.table.folder_lookup();
    let profiles = state.search.profile_ids_with(&mut folders, deadline);
    Some(VolumeLookup { folders, profiles })
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
