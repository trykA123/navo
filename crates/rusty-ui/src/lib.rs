#![forbid(unsafe_code)]

mod app;
mod assets;
pub mod file_list;
pub mod fkey_bar;
mod format;
pub mod pane_chrome;
pub mod prelude;
pub mod primitives;
pub mod status_bar;
pub mod tokens;

#[cfg(test)]
mod test_window;

pub use app::{UiError, application, init};
pub use assets::Assets;
pub use gpui_kit::open_window;
pub use gpui_kit::prelude::FluentBuilder;
pub use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext, Bounds, ClickEvent, Context, Div, ElementId,
    Entity, FocusHandle, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Pixels,
    Point, Render, SharedString, StatefulInteractiveElement, Styled, TitlebarOptions, Window,
    WindowBounds, WindowOptions, div, px, relative, size,
};
