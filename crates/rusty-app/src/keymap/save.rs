use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use rusty_store::{KeymapEdit, KeymapFile, KeymapFileError};
use rusty_ui::commands::{GLOBAL, by_name};
use rusty_ui::shortcuts::{Edit, SaveKeymap};

use super::{FILE, checked};

#[must_use]
pub fn saver(path: &Path) -> SaveKeymap {
    let file = Mutex::new(KeymapFile::new(path));
    Arc::new(move |edits| {
        let changes: Vec<KeymapEdit> = edits.iter().map(change_of).collect();
        let mut file = file.lock().unwrap_or_else(PoisonError::into_inner);
        let saved = file
            .save(&changes, checked)
            .map_err(|error| describe(&error));
        if let Err(problem) = &saved {
            tracing::error!(%problem, "keymap change not saved");
        }
        saved
    })
}

fn change_of(edit: &Edit) -> KeymapEdit {
    match edit {
        Edit::Set {
            command,
            context,
            keys,
        } => KeymapEdit::Set {
            command: (*command).to_owned(),
            context: context.clone(),
            home: by_name(command)
                .map_or(GLOBAL, |spec| spec.context)
                .to_owned(),
            keys: keys.clone(),
        },
        Edit::Reset { command } => KeymapEdit::Reset {
            command: (*command).to_owned(),
        },
    }
}

fn describe(error: &KeymapFileError) -> String {
    match error {
        KeymapFileError::FileInvalid(message) | KeymapFileError::Rejected(message) => {
            message.clone()
        }
        other => format!("{FILE}: {other}"),
    }
}
