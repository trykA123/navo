//! Opens PDFs through the pooled helper process; on Windows each helper starts inside its Job
//! Object and receives a read-only handle, never a path.

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

/// Starts the helper suspended inside its AppContainer and a 1 GiB Job Object, then resumes it,
/// so it never runs unconfined; `spawn_confined` leaves no live process when it fails (S2a1, S2c2).
#[cfg(windows)]
fn spawn_in_job(spec: &navo_pdf::HelperSpec) -> std::io::Result<navo_pdf::Spawned> {
    const HELPER_MEMORY: usize = 1 << 30;
    let limits = navo_shell::HelperLimits {
        memory_bytes: HELPER_MEMORY,
    };
    let child =
        navo_shell::spawn_confined(&spec.program, &spec.args, limits).map_err(unconfined)?;
    Ok(navo_pdf::Spawned {
        process: Box::new(JobProcess(child.process)),
        input: Box::new(child.stdin),
        output: Box::new(child.stdout),
        guard: None,
    })
}

/// Turns the shell's AppContainer refusal into navo-pdf's `Unconfined` ("cannot open safely"),
/// keeping whether a retry may help; any other spawn error passes through unchanged.
#[cfg(windows)]
fn unconfined(error: std::io::Error) -> std::io::Error {
    let refusal = error
        .get_ref()
        .and_then(|e| e.downcast_ref::<navo_shell::ContainerError>());
    let Some(refusal) = refusal else {
        return error;
    };
    let permanence = if refusal.is_transient() {
        navo_pdf::Permanence::Transient
    } else {
        navo_pdf::Permanence::Permanent
    };
    std::io::Error::other(navo_pdf::Unconfined {
        reason: refusal.to_string(),
        permanence,
    })
}

/// The confined helper as navo-pdf's process; it owns the Job, so dropping it kills the helper.
#[cfg(windows)]
struct JobProcess(navo_shell::ConfinedProcess);

#[cfg(windows)]
impl navo_pdf::HelperProcess for JobProcess {
    /// Terminates the helper; `Ok` when it has already exited.
    fn kill(&mut self) -> std::io::Result<()> {
        self.0.kill()
    }

    /// Polls whether the helper has exited (`true` = exited).
    fn try_wait(&mut self) -> std::io::Result<bool> {
        self.0.try_wait()
    }

    /// The full-access process handle, so the S1b share hook can duplicate into it.
    fn process_handle(&self) -> std::os::windows::io::BorrowedHandle<'_> {
        self.0.process_handle()
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
