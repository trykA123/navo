#![forbid(unsafe_code)]

pub mod activity;
pub mod activity_strip;
pub mod app_menu;
pub mod browser;
pub mod clip_palette;
pub mod command_center;
pub mod commands;
pub mod compare_diff;
pub mod compare_keys;
pub mod compare_list;
mod compare_visible;
pub mod crash_notice;
pub mod drag;
pub mod file_list;
pub mod fkey_bar;
pub mod git;
pub mod job_card;
pub mod job_row;
mod list_actions;
pub mod modifier_hints;
pub mod multi_rename;
mod name_match;
pub mod pane_chrome;
#[cfg(feature = "tester")]
pub mod perf;
#[cfg(not(feature = "tester"))]
#[path = "perf_off.rs"]
pub mod perf;
pub mod primitives;
pub mod prompt_dialog;
pub mod recently_deleted;
pub mod rename;
pub mod settings;
pub mod setup;
pub mod shortcuts;
pub mod side_sheet;
pub mod status_bar;
pub mod title_bar;
pub mod window_view;

#[cfg(test)]
mod syntax_lexer_tests;
#[cfg(test)]
use sidewise_kit::test_pointer;
#[cfg(test)]
mod test_window;

pub use commands::kit_keys;
pub use gpui_kit::open_window;
pub use gpui_kit::prelude::FluentBuilder;
pub use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext, Bounds, ClickEvent, Context, Div, ElementId,
    Entity, FocusHandle, Focusable, FontWeight, Hsla, InteractiveElement, IntoElement,
    ParentElement, Pixels, Point, Render, SharedString, StatefulInteractiveElement, Styled,
    TitlebarOptions, Window, WindowBounds, WindowOptions, div, px, relative, size,
};
pub use sidewise_kit::{Assets, UiError, application, init};
pub(crate) use sidewise_kit::{assets, dialog_focus, format, host_os};
pub use sidewise_kit::{confirm_dialog, context_menu, kit_probe, languages, prelude, tokens};
pub use title_bar::{BrandMark, HeaderSearch, NavoTitleBar, window_options};
