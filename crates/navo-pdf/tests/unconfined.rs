//! A spawn hook that cannot confine the helper (S2c2): the client reports "cannot open safely",
//! and a permanent refusal is remembered so the helper is never respawned in a loop.

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use navo_pdf::protocol::PdfError;
use navo_pdf::{Document, HelperSpec, Launcher, Permanence, Unconfined};

const FAKE: &str = env!("CARGO_BIN_EXE_navo-pdf-fake-helper");
const REASON: &str = "AppContainer grant failed at WriteDacl";

/// A launcher whose hook always refuses with `permanence`, and the count of hook calls.
fn refusing(permanence: Permanence) -> (Launcher, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let launcher = Launcher::new(FAKE, [])
        .with_timeout(Duration::from_secs(5))
        .with_spawn(Arc::new(move |_: &HelperSpec| {
            seen.fetch_add(1, Ordering::SeqCst);
            Err(io::Error::other(Unconfined {
                reason: REASON.into(),
                permanence,
            }))
        }));
    (launcher, calls)
}

fn open(launcher: &Launcher) -> Option<PdfError> {
    Document::open(launcher, Path::new("a.pdf"), None).err()
}

#[test]
fn an_unconfined_helper_is_cannot_open_safely_not_no_helper() {
    let (launcher, _) = refusing(Permanence::Transient);
    assert_eq!(open(&launcher), Some(PdfError::Unconfined(REASON.into())));
}

#[test]
fn a_permanent_refusal_never_calls_the_hook_again() {
    let (launcher, calls) = refusing(Permanence::Permanent);
    assert_eq!(launcher.prewarm(), Err(PdfError::Unconfined(REASON.into())));
    assert_eq!(open(&launcher), Some(PdfError::Unconfined(REASON.into())));
    assert_eq!(
        open(&launcher.clone()),
        Some(PdfError::Unconfined(REASON.into()))
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn a_transient_refusal_tries_once_per_open_and_never_loops() {
    let (launcher, calls) = refusing(Permanence::Transient);
    assert_eq!(open(&launcher), Some(PdfError::Unconfined(REASON.into())));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(open(&launcher), Some(PdfError::Unconfined(REASON.into())));
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn any_other_hook_error_stays_no_helper() {
    let launcher = Launcher::new(FAKE, [])
        .with_spawn(Arc::new(|_: &HelperSpec| Err(io::Error::other("no job"))));
    assert_eq!(
        open(&launcher),
        Some(PdfError::NoHelper("cannot confine it: no job".into()))
    );
}

#[test]
fn a_hook_reporting_a_missing_engine_is_no_engine_and_sets_no_latch() {
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let launcher = Launcher::new(FAKE, []).with_spawn(Arc::new(move |_: &HelperSpec| {
        seen.fetch_add(1, Ordering::SeqCst);
        Err(io::Error::other(PdfError::NoEngine("pdfium.dll".into())))
    }));
    assert_eq!(
        open(&launcher),
        Some(PdfError::NoEngine("pdfium.dll".into()))
    );
    assert_eq!(
        open(&launcher),
        Some(PdfError::NoEngine("pdfium.dll".into()))
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}
