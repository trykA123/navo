//! Sidewise's compare screens: the folder-compare model, file diff, list UI and keys.
//! Moved out of navo-ui; navo-ui re-exports each module under its old path.
#![forbid(unsafe_code)]

pub mod browser;
pub mod commands {
    /// The compare action types, which the keymap, menus and tests name.
    pub mod compare {
        pub use super::compare_actions::*;
    }
    pub mod compare_actions;
}
pub mod compare_diff;
pub mod compare_keys;
pub mod compare_list;
pub mod compare_visible;
pub mod list_actions;

#[allow(
    unused_imports,
    reason = "NV still reaches these modules through sidewise-ui"
)]
pub(crate) use sidewise_kit::{
    assets, confirm_dialog, context_menu, file_icons, format, languages, prelude, primitives,
    row_look, tokens,
};
