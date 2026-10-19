use std::ffi::OsString;
use std::io;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use rusty_pdf::fake::{CRASH_PAGE, HANG_PAGE, PAGES, PASSWORD};
use rusty_pdf::protocol::{PdfError, RenderRequest, Rotation};
use rusty_pdf::{Document, Launcher};

const FAKE: &str = env!("CARGO_BIN_EXE_rusty-pdf-fake-helper");

fn launcher() -> Launcher {
    Launcher::new(FAKE, []).with_timeout(Duration::from_secs(10))
}

fn launcher_with(mode: &str) -> Launcher {
    Launcher::new(FAKE, [OsString::from(mode)]).with_timeout(Duration::from_millis(500))
}

fn page(page: u32) -> RenderRequest {
    RenderRequest {
        page,
        width: 40,
        rotation: Rotation::None,
    }
}

fn open(launcher: &Launcher, name: &str, password: Option<&str>) -> Result<Document, PdfError> {
    Document::open(launcher, Path::new(name), password)
}

#[test]
fn a_document_opens_renders_and_reads_text() {
    let mut document = open(&launcher(), "a.pdf", None).unwrap();
    assert_eq!(document.pages(), PAGES);
    assert_eq!(document.page_count().unwrap(), PAGES);
    let bitmap = document.render(page(2)).unwrap();
    assert_eq!((bitmap.width(), bitmap.height()), (40, 80));
    assert_eq!(&bitmap.rgba()[..4], &[2, 0, 0, 255]);
    assert_eq!(document.text(3).unwrap().text(), "page 3");
    document.close();
}

#[test]
fn passwords_are_asked_for_and_checked() {
    let launcher = launcher();
    assert_eq!(
        open(&launcher, "locked.pdf", None).err(),
        Some(PdfError::NeedsPassword)
    );
    assert_eq!(
        open(&launcher, "locked.pdf", Some("guess")).err(),
        Some(PdfError::WrongPassword)
    );
    assert!(open(&launcher, "locked.pdf", Some(PASSWORD)).is_ok());
}

#[test]
fn a_corrupt_document_is_reported() {
    assert_eq!(
        open(&launcher(), "broken.pdf", None).err(),
        Some(PdfError::Corrupt)
    );
}

#[test]
fn a_hung_render_times_out_and_the_next_request_restarts_the_helper() {
    let launcher = launcher().with_timeout(Duration::from_millis(400));
    let mut document = open(&launcher, "a.pdf", None).unwrap();
    let started = Instant::now();
    assert_eq!(document.render(page(HANG_PAGE)), Err(PdfError::Timeout));
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(document.render(page(1)).unwrap().width(), 40);
}

#[test]
fn a_crashed_helper_is_reported_and_restarted() {
    let mut document = open(&launcher(), "locked.pdf", Some(PASSWORD)).unwrap();
    assert_eq!(document.render(page(CRASH_PAGE)), Err(PdfError::HelperDied));
    assert_eq!(document.text(0).unwrap().text(), "page 0");
}

#[test]
fn a_helper_that_never_answers_times_out_on_open() {
    assert_eq!(
        open(&launcher_with("--silent"), "a.pdf", None).err(),
        Some(PdfError::Timeout)
    );
}

#[test]
fn a_helper_that_writes_garbage_is_a_protocol_error() {
    let error = open(&launcher_with("--garbage"), "a.pdf", None).err();
    assert!(matches!(error, Some(PdfError::Protocol(_))), "{error:?}");
}

#[test]
fn a_missing_helper_program_is_reported() {
    let launcher = Launcher::new("/nonexistent/ferrite-helper", []);
    assert!(matches!(
        open(&launcher, "a.pdf", None).err(),
        Some(PdfError::NoHelper(_))
    ));
}

#[test]
fn each_start_is_confined_and_a_failed_confinement_refuses_to_run() {
    let starts = Arc::new(AtomicUsize::new(0));
    let counter = starts.clone();
    let confined = launcher().with_confinement(Arc::new(move |_child| {
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(Box::new(()))
    }));
    let mut document = open(&confined, "a.pdf", None).unwrap();
    let _ = document.render(page(CRASH_PAGE));
    document.render(page(0)).unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 2);
    let refused = launcher().with_confinement(Arc::new(|_child| Err(io::Error::other("no job"))));
    assert!(matches!(
        open(&refused, "a.pdf", None).err(),
        Some(PdfError::NoHelper(_))
    ));
}

#[test]
fn a_document_moves_to_a_worker_thread() {
    let document = open(&launcher(), "a.pdf", None).unwrap();
    let pages = std::thread::spawn(move || {
        let mut document = document;
        document.page_count()
    })
    .join()
    .unwrap();
    assert_eq!(pages, Ok(PAGES));
}
