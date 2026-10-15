use std::fmt;

use gpui_kit::{App, Application};

use crate::assets::{Assets, load_fonts};
use crate::tokens::{self, Mode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiError {
    Fonts(String),
}

impl fmt::Display for UiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fonts(message) => write!(formatter, "loading the bundled fonts: {message}"),
        }
    }
}

impl std::error::Error for UiError {}

pub fn application() -> Application {
    gpui_kit::application().with_assets(Assets)
}

pub fn init(mode: Mode, cx: &mut App) -> Result<(), UiError> {
    gpui_kit::init(cx);
    crate::languages::register_languages();
    let fonts = load_fonts(cx);
    tokens::apply(mode, cx);
    fonts
}
