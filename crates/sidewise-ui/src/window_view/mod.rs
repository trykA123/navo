//! The window's shared UI pieces: the compare screen and the viewer parts Sidewise draws.
pub mod compare_screen;
pub mod viewer;

#[doc(hidden)]
pub const ONE_PANE_BELOW: gpui_kit::Pixels = gpui_kit::px(900.);

pub use compare_screen::{
    CompareFiles, CompareScreen, DiffSlot, LookDefaults, LookPreset, ScreenEvent, ScreenParts,
    SettingKey,
};
#[doc(hidden)]
pub use compare_screen::{items_of, neighbour};
#[doc(hidden)]
pub type ScreenOwner = compare_screen::CompareScreen;
