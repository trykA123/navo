use std::time::{Duration, Instant};

use rusty_index::pipe::{FolderTotals, FolderTotalsReply, Reply, folder_paths_within_limits};
use rusty_index::table::{FolderLookup, FolderLookupExpired};

use super::answer::{SharedVolume, State, read_before};

pub(super) fn answer(volumes: &[SharedVolume], paths: &[String]) -> Reply {
    answer_before(volumes, paths, Instant::now() + Duration::from_millis(20))
}

fn answer_before(volumes: &[SharedVolume], paths: &[String], deadline: Instant) -> Reply {
    if !folder_paths_within_limits(paths) {
        return Reply {
            totals: Some(FolderTotalsReply {
                totals: Vec::new(),
                busy: true,
            }),
            ..Reply::default()
        };
    }
    let mut totals = vec![None; paths.len()];
    let mut busy = false;
    for volume in volumes {
        if Instant::now() >= deadline {
            busy = true;
            break;
        }
        let Some(state) = read_before(&volume.shared, deadline) else {
            busy = true;
            continue;
        };
        let mut lookup = state
            .index
            .as_ref()
            .map(|index| index.table.folder_lookup());
        for (path, total) in paths.iter().zip(&mut totals) {
            match total_for_path(&mut lookup, &state, path, deadline) {
                Ok(Some(found)) => *total = Some(found),
                Ok(None) => {}
                Err(_) => {
                    busy = true;
                    break;
                }
            }
        }
    }
    Reply {
        totals: Some(FolderTotalsReply { totals, busy }),
        ..Reply::default()
    }
}

fn total_for_path(
    lookup: &mut Option<FolderLookup<'_>>,
    state: &State,
    path: &str,
    deadline: Instant,
) -> Result<Option<FolderTotals>, FolderLookupExpired> {
    if Instant::now() >= deadline {
        return Err(FolderLookupExpired);
    }
    let root = state.status.root.trim_end_matches(['/', '\\']);
    if !path
        .get(..root.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(root))
        || !matches!(path.as_bytes().get(root.len()), None | Some(b'/' | b'\\'))
    {
        return Ok(None);
    }
    let complete =
        state.status.progress.is_none() && state.status.errors.is_empty() && state.index.is_some();
    let id = lookup
        .as_mut()
        .map(|lookup| lookup.find_folder(path, deadline))
        .transpose()?
        .flatten();
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
