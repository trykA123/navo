use std::ffi::OsString;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use navo_core::Location;
use navo_pdf::protocol::PdfError;
use navo_pdf::{Document, Launcher};
use navo_ui::window_view::PdfPages;

/// Puts each helper in a memory-capped Job Object and shares the PDF as a read-only handle,
/// so the helper never receives a path (S1c).
#[cfg(windows)]
fn confined(launcher: Launcher) -> Launcher {
    let share: navo_pdf::ShareFile = std::sync::Arc::new(navo_shell::share_read_handle);
    launcher
        .with_share(share)
        .with_spawn(std::sync::Arc::new(spawn_in_job))
}

/// Starts the helper, then assigns it to a 1 GiB Job Object. On a Job error it kills and reaps
/// the child before it returns `Err`, so a failed spawn leaves no live helper (S2a contract).
#[cfg(windows)]
fn spawn_in_job(spec: &navo_pdf::HelperSpec) -> std::io::Result<navo_pdf::Spawned> {
    const HELPER_MEMORY: usize = 1 << 30;
    let mut child = spec.command().spawn()?;
    let limits = navo_shell::HelperLimits {
        memory_bytes: HELPER_MEMORY,
    };
    match navo_shell::confine_helper(&child, limits) {
        Ok(job) => navo_pdf::Spawned::from_child(child, Some(Box::new(job))),
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(error)
        }
    }
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
