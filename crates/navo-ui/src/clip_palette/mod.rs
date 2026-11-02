mod actions;
#[cfg(test)]
mod actions_tests;
mod age;
mod chrome;
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
mod preview;
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
    Origin, SavedDraft, Scope, SecretText,
};
pub use keys::bind_keys;
pub use view::{ClipPalette, ClipPaletteEvent, ClipPaletteProps, Place};
pub use window::{PopUpRequest, summon};
