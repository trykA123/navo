use std::ffi::OsString;

use rusty_core::Location;
use rusty_pdf::protocol::PdfError;
use rusty_pdf::{Document, Launcher};
use rusty_ui::window_view::PdfPages;

#[cfg(windows)]
fn confined(launcher: Launcher) -> Launcher {
    const HELPER_MEMORY: usize = 1 << 30;
    launcher.with_confinement(std::sync::Arc::new(|child: &std::process::Child| {
        let limits = rusty_shell::HelperLimits {
            memory_bytes: HELPER_MEMORY,
        };
        rusty_shell::confine_helper(child, limits).map(|job| Box::new(job) as Box<dyn Send>)
    }))
}

#[cfg(not(windows))]
fn confined(launcher: Launcher) -> Launcher {
    launcher
}

fn launcher() -> Result<Launcher, PdfError> {
    let exe = std::env::current_exe().map_err(|error| PdfError::NoHelper(error.to_string()))?;
    Ok(confined(Launcher::new(
        exe,
        [OsString::from("--pdf-helper")],
    )))
}

pub(super) fn open(
    location: &Location,
    password: Option<&str>,
) -> Result<Box<dyn PdfPages>, PdfError> {
    let path = super::path_of(location).map_err(PdfError::Unreadable)?;
    let document = Document::open(&launcher()?, &path, password)?;
    Ok(Box::new(document))
}
