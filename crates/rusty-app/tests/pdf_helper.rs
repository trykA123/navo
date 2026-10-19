use std::ffi::OsString;
use std::path::{Path, PathBuf};

use rusty_pdf::protocol::{Bitmap, PdfError, RenderRequest, Rotation};
use rusty_pdf::{Document, Launcher, library_beside};

const FERRITE: &str = env!("CARGO_BIN_EXE_ferrite");

fn launcher() -> Launcher {
    let launcher = Launcher::new(FERRITE, [OsString::from("--pdf-helper")]);
    #[cfg(windows)]
    let launcher = launcher.with_confinement(std::sync::Arc::new(|child: &std::process::Child| {
        let limits = rusty_shell::HelperLimits {
            memory_bytes: 1 << 30,
        };
        rusty_shell::confine_helper(child, limits).map(|job| Box::new(job) as Box<dyn Send>)
    }));
    launcher
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../rusty-pdf/tests/fixtures/tiny.pdf")
}

fn expects_pdfium() -> bool {
    let beside = Path::new(FERRITE).parent().map(library_beside);
    std::env::var_os("FERRITE_EXPECT_PDFIUM").is_some() || beside.is_some_and(|path| path.is_file())
}

fn pixel(bitmap: &Bitmap, x: u32, y: u32) -> &[u8] {
    let start = ((y * bitmap.width() + x) * 4) as usize;
    &bitmap.rgba()[start..start + 4]
}

fn render(document: &mut Document, width: u32, rotation: Rotation) -> Result<Bitmap, PdfError> {
    document.render(RenderRequest {
        page: 0,
        width,
        rotation,
    })
}

fn check_rendering(mut document: Document) -> Result<(), PdfError> {
    assert_eq!(document.pages(), 1);
    let page = render(&mut document, 400, Rotation::None)?;
    assert_eq!((page.width(), page.height()), (400, 200));
    assert_eq!(page.rgba().len(), 400 * 200 * 4);
    assert_eq!(pixel(&page, 100, 100), &[0, 0, 0, 255]);
    assert_eq!(pixel(&page, 4, 4), &[255, 255, 255, 255]);
    let turned = render(&mut document, 200, Rotation::Quarter)?;
    assert_eq!((turned.width(), turned.height()), (200, 400));
    assert!(document.text(0)?.text().contains("Ferrite"));
    assert_eq!(
        document.text(1).err(),
        Some(PdfError::NoSuchPage { page: 1, count: 1 })
    );
    document.close();
    Ok(())
}

#[test]
fn the_helper_mode_renders_page_one_or_names_the_missing_engine() {
    match Document::open(&launcher(), &fixture(), None) {
        Ok(document) => check_rendering(document).unwrap(),
        Err(PdfError::NoEngine(message)) => {
            assert!(!expects_pdfium(), "pdfium is present but failed: {message}");
            assert!(message.starts_with("PDF view needs pdfium"), "{message}");
        }
        Err(other) => panic!("unexpected error: {other}"),
    }
}

#[test]
fn a_file_that_is_not_a_pdf_is_refused_without_killing_the_helper() {
    let launcher = launcher();
    let not_pdf = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let error = Document::open(&launcher, &not_pdf, None).err();
    if expects_pdfium() {
        assert_eq!(error, Some(PdfError::Corrupt));
    } else {
        assert!(matches!(error, Some(PdfError::NoEngine(_))), "{error:?}");
    }
}
