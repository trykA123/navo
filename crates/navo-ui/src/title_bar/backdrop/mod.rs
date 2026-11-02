#[cfg(not(windows))]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(test)]
mod tests;

use std::sync::OnceLock;

use gpui_kit::WindowBackgroundAppearance;

#[cfg(not(windows))]
use unix as os;
#[cfg(windows)]
use windows as os;

const FIRST_MICA_BUILD: u32 = 22621;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GradientLayer {
    Root,
    BelowTitleBar,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Backdrop {
    pub window: WindowBackgroundAppearance,
    pub gradient: GradientLayer,
}

pub(crate) fn backdrop_for(windows_build: Option<u32>) -> Backdrop {
    match windows_build {
        Some(build) if build >= FIRST_MICA_BUILD => Backdrop {
            window: WindowBackgroundAppearance::MicaBackdrop,
            gradient: GradientLayer::BelowTitleBar,
        },
        _ => Backdrop {
            window: WindowBackgroundAppearance::Opaque,
            gradient: GradientLayer::Root,
        },
    }
}

pub(crate) fn system_backdrop() -> Backdrop {
    static BACKDROP: OnceLock<Backdrop> = OnceLock::new();
    *BACKDROP.get_or_init(|| backdrop_for(os::windows_build()))
}
