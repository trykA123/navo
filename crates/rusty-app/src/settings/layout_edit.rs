use std::sync::{Arc, Mutex, PoisonError};

use rusty_store::{SettingsStore, remove_layout_preset, replace_layout_preset, save_layout_preset};
use rusty_ui::window_view::{EditLayout, LayoutEdit, SaveReport, no_layout_edits};

use super::{describe, values_of};

pub(super) fn hook(store: Option<Arc<Mutex<SettingsStore>>>) -> EditLayout {
    let Some(store) = store else {
        return no_layout_edits();
    };
    Arc::new(move |edit| {
        let mut store = store.lock().unwrap_or_else(PoisonError::into_inner);
        let saved = store
            .edit_text(|text| match edit {
                LayoutEdit::Save { name, tiles } => save_layout_preset(text, name, tiles),
                LayoutEdit::Replace { name, tiles } => replace_layout_preset(text, name, tiles),
                LayoutEdit::Remove { name } => remove_layout_preset(text, name),
            })
            .map_err(|error| describe(&error))?;
        Ok(SaveReport {
            error: saved.durability_error.as_ref().map(describe),
            generation: store.generation(),
            ..SaveReport::saved(values_of(&store))
        })
    })
}
