use rusty_store::{JobId, JobState, UndoKinds, Undoable};

use super::{KIND, MOVE_KIND, RETAINED};
use crate::files::now_ms;
use crate::jobs::{SharedJournal, lock};

pub(crate) fn claim_undo(
    journal: &SharedJournal,
    job: JobId,
    state: JobState,
) -> Result<(), String> {
    let mut journal = lock(journal);
    let latest = journal
        .newest_undoable(UndoKinds {
            rename: KIND,
            moves: MOVE_KIND,
            retained: &RETAINED,
            now_ms: now_ms(),
        })
        .map_err(|error| error.to_string())?;
    let latest = latest.map(|offer| match offer {
        Undoable::Rename(offer) => offer.id,
        Undoable::Retained { id, .. } => id,
    });
    if latest != Some(job) {
        return Err(rusty_l10n::tr("undo-nothing"));
    }
    journal
        .finish(job, state)
        .map_err(|error| error.to_string())
}
