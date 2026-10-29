use std::ffi::OsString;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

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

static SHARED: OnceLock<Result<Launcher, PdfError>> = OnceLock::new();
static WARMING: AtomicBool = AtomicBool::new(false);

fn build() -> Result<Launcher, PdfError> {
    let exe = std::env::current_exe().map_err(|error| PdfError::NoHelper(error.to_string()))?;
    Ok(confined(Launcher::new(
        exe,
        [OsString::from("--pdf-helper")],
    )))
}

fn launcher() -> Result<Launcher, PdfError> {
    SHARED.get_or_init(build).clone()
}

pub(crate) fn prewarm_after(delay: Duration) {
    schedule(&WARMING, delay, || {
        if let Err(error) = launcher().and_then(|launcher| launcher.prewarm()) {
            tracing::debug!(%error, "the PDF helper did not warm up");
        }
    });
}

pub(crate) fn shutdown() {
    if let Ok(launcher) = launcher() {
        launcher.shutdown();
    }
}

fn schedule(
    started: &AtomicBool,
    delay: Duration,
    job: impl FnOnce() + Send + 'static,
) -> Option<JoinHandle<()>> {
    if started.swap(true, Ordering::SeqCst) {
        return None;
    }
    thread::Builder::new()
        .name("navo-pdf-warm".into())
        .spawn(move || {
            thread::sleep(delay);
            job();
        })
        .inspect_err(|error| tracing::warn!(%error, "cannot start the PDF warm-up"))
        .ok()
}

pub(super) fn open(
    location: &Location,
    password: Option<&str>,
) -> Result<Box<dyn PdfPages>, PdfError> {
    let path = super::path_of(location).map_err(PdfError::Unreadable)?;
    let document = Document::open(&launcher()?, &path, password)?;
    Ok(Box::new(document))
}

#[cfg(test)]
mod tests;
