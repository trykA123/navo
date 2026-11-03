mod actions;
#[cfg(test)]
mod actions_tests;
mod age;
mod chrome;
mod edit_form;
mod edit_form_actions;
mod edit_form_choices;
mod edit_form_keys;
#[cfg(test)]
mod edit_form_tests;
mod edit_form_view;
mod empty;
mod hooks;
#[cfg(test)]
mod intro_tests;
mod keys;
mod list;
mod manage;
mod manage_kinds;
#[cfg(test)]
mod manage_tests;
mod manage_view;
mod menu;
mod note_editor;
mod note_pane;
mod notes;
#[cfg(test)]
mod notes_tests;
mod password;
mod placement;
mod preview;
mod refresh;
mod repoint;
mod rows;
mod secret;
#[cfg(test)]
mod secret_tests;
mod state;
mod status;
#[cfg(test)]
mod tests;
mod view;
pub(crate) mod window;

pub use hooks::{
    ClipCounts, ClipId, ClipKind, ClipMode, ClipResults, ClipRow, ClipboardHooks, How, NoClipboard,
    Origin, SavedDraft, SavedEdit, Scope, SecretText,
};
pub use keys::bind_keys;
pub use note_editor::FolderProbe;
pub use placement::{PopUpPlacement, PopUpPosition, PopUpRect};
pub use view::{ClipPalette, ClipPaletteEvent, ClipPaletteProps, Place};
pub use window::{PopUpRequest, summon};
