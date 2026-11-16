//! Sidewise design-system kit: tokens, languages, fonts, assets and app start-up.
//! Moved out of navo-ui; navo-ui re-exports each module under its old path.
#![forbid(unsafe_code)]

pub mod app;
pub mod assets;
pub mod confirm_dialog;
pub mod context_menu;
pub mod dialog_focus;
pub mod file_icons;
pub mod format;
pub mod host_os;
pub mod kit_probe;
pub mod languages;
pub mod prelude;
pub mod primitives;
pub mod row_look;
pub mod shown;
#[cfg(any(test, feature = "test-support"))]
pub mod test_pointer;
#[cfg(any(test, feature = "test-support"))]
pub mod test_window;
pub mod tokens;

pub use app::{UiError, application, init};
pub use assets::Assets;
pub use gpui_kit::open_window;
pub use gpui_kit::prelude::FluentBuilder;
pub use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext, Bounds, ClickEvent, Context, Div, ElementId,
    Entity, FocusHandle, Focusable, FontWeight, Hsla, InteractiveElement, IntoElement,
    ParentElement, Pixels, Point, Render, SharedString, StatefulInteractiveElement, Styled,
    TitlebarOptions, Window, WindowBounds, WindowOptions, div, px, relative, size,
};
