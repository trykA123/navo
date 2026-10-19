#![forbid(unsafe_code)]

mod client;
#[doc(hidden)]
pub mod fake;
mod pdfium;
pub mod protocol;
mod serve;

pub use client::{Confine, DEFAULT_TIMEOUT, Document, Launcher};
pub use pdfium::{MAX_FILE_BYTES, PdfiumEngine, library_beside};
pub use serve::{Engine, Unavailable, clip, serve};

pub const PDFIUM_VERSION: &str = "chromium/8076";

#[cfg(test)]
mod tests {
    #[test]
    fn the_version_matches_the_pdfium_pinned_in_ci() {
        let workflow = include_str!("../../../.github/workflows/ci.yml");
        let pin = format!("PDFIUM_TAG: {}\n", super::PDFIUM_VERSION);
        assert!(workflow.contains(&pin), "ci.yml must pin {pin}");
    }
}
