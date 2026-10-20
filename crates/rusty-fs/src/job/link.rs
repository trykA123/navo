use rusty_core::{FsError, FsErrorKind, Location};

use super::StepOutcome;
use super::aside::presence;
use crate::{EntryKind, ProviderOps};

pub(super) fn undo_link(
    ops: &dyn ProviderOps,
    at: &Location,
    target_of: &Location,
) -> Result<StepOutcome, FsError> {
    let Some(found) = presence(ops, at)? else {
        return Ok(StepOutcome::Skipped);
    };
    let shortcut = makes_shortcut_files(at);
    let ours = if shortcut {
        found.kind == EntryKind::File && is_shortcut_name(at)
    } else {
        found.kind == EntryKind::Link
    };
    if !ours || !points_at(ops, at, target_of)? {
        return Ok(StepOutcome::Skipped);
    }
    let result = if shortcut {
        ops.remove_file(at)
    } else {
        ops.remove_link(at)
    };
    match result {
        Ok(()) => Ok(StepOutcome::Done),
        Err(error) if error.kind == FsErrorKind::NotFound => Ok(StepOutcome::Skipped),
        Err(error) => Err(error),
    }
}

fn points_at(ops: &dyn ProviderOps, at: &Location, target_of: &Location) -> Result<bool, FsError> {
    match ops.link_target(at) {
        Ok(found) => Ok(found.is_some_and(|found| same_target(&found, target_of))),
        Err(error) if error.kind == FsErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn same_target(found: &Location, wanted: &Location) -> bool {
    found == wanted
        || (cfg!(windows)
            && found.is_local()
            && found
                .path()
                .to_text()
                .eq_ignore_ascii_case(&wanted.path().to_text()))
}

fn makes_shortcut_files(at: &Location) -> bool {
    cfg!(windows) && at.is_local()
}

fn is_shortcut_name(at: &Location) -> bool {
    at.name().is_some_and(|name| {
        name.to_text()
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("lnk"))
    })
}
