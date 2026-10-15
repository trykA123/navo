#![forbid(unsafe_code)]

mod app;
mod assets;
pub mod prelude;
pub mod primitives;
pub mod tokens;

pub use app::{UiError, application, init};
pub use assets::Assets;
pub use gpui_kit::open_window;
