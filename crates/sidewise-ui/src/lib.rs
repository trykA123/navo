//! Sidewise's compare screens and viewer parts, moved out of navo-ui.
//! Navo re-exports each module under its old path.
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
pub mod window_view;

pub(crate) use sidewise_kit::{
    assets, confirm_dialog, context_menu, file_icons, format, languages, prelude, primitives,
    row_look, tokens,
};
