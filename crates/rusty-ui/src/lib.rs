#![forbid(unsafe_code)]

mod app;
mod assets;
pub mod browser;
pub mod prelude;
pub mod primitives;
pub mod title_bar;
pub mod tokens;

pub use app::{UiError, application, init};
pub use assets::Assets;
pub use gpui_kit::open_window;
pub use title_bar::{FerriteTitleBar, HeaderSearch, window_options};
