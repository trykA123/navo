use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use gpui_kit::{AnyWeakView, AnyWindowHandle, Entity, TestAppContext};
use image::{ImageFormat, Rgba, RgbaImage};
use rusty_core::Location;
use rusty_core::paging::{ByteSource, LineIndex};
use rusty_diag::components::{MemoryUse, Tally};
use rusty_fs::archive::{ArchiveEntry, ArchiveFormat, ArchiveListing};
use rusty_pdf::protocol::{Bitmap, Outline, PageLinks, PageText, PdfError, RenderRequest};

use super::press;
use crate::primitives::{GPU_IMAGES, read_tally};
use crate::window_view::{FerriteWindow, FileHooks, FileText, Head, Paged, PdfPages};

const INTER: &[u8] = include_bytes!("../../../fonts/Inter-Regular.ttf");
const ZIP_HEAD: &[u8] = b"PK\x03\x04\x14\x00\x00\x00\x08\x00";
const PDF_HEAD: &[u8] = b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n1 0 obj\n\x00";
const SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12"><rect width="12" height="12" fill="#f00"/></svg>"##;
pub(in crate::window_view) const PDF_PAGES: u32 = 12;

struct Handle(Arc<AtomicUsize>);

impl Handle {
    fn open(count: &Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self(count.clone())
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

struct OpenBytes {
    bytes: Vec<u8>,
    _handle: Handle,
}

impl ByteSource for OpenBytes {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.bytes.as_slice().read_at(offset, buf)
    }
}

struct OpenPages {
    _handle: Handle,
}

impl PdfPages for OpenPages {
    fn page_count(&self) -> u32 {
        PDF_PAGES
    }

    fn render(&mut self, request: RenderRequest) -> Result<Bitmap, PdfError> {
        let height = request.width * 4 / 3;
        let rgba = vec![255; request.width as usize * height as usize * 4];
        Bitmap::new(request.width, height, rgba).map_err(|_| PdfError::TooLarge)
    }

    fn text(&mut self, _: u32) -> Result<PageText, PdfError> {
        PageText::new(600., 800., Vec::new()).map_err(|_| PdfError::Corrupt)
    }

    fn links(&mut self, _: u32) -> Result<PageLinks, PdfError> {
        PageLinks::new(600., 800., Vec::new()).map_err(|_| PdfError::Corrupt)
    }

    fn outline(&mut self) -> Result<Outline, PdfError> {
        Outline::new(Vec::new()).map_err(|_| PdfError::Corrupt)
    }
}

fn png() -> Vec<u8> {
    let picture = RgbaImage::from_pixel(64, 48, Rgba([40, 90, 160, 255]));
    let mut out = Cursor::new(Vec::new());
    picture
        .write_to(&mut out, ImageFormat::Png)
        .expect("png encodes");
    out.into_inner()
}

pub(in crate::window_view) fn samples() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("blob.bin", (0..=255u8).cycle().take(4096).collect()),
        ("bundle.zip", ZIP_HEAD.to_vec()),
        ("config.yaml", b"name: shop\nitems:\n  - first\n".to_vec()),
        ("face.ttf", INTER.to_vec()),
        ("feed.xml", b"<feed><item>first</item></feed>".to_vec()),
        ("manual.pdf", PDF_HEAD.repeat(64)),
        ("notes.txt", b"fn main() {}\nlet second = 2;\n".to_vec()),
        ("photo.png", png()),
        ("readme.md", b"# Navo\n\nA file manager.\n".to_vec()),
        ("sales.csv", b"date,units\n2026-07-01,3\n".to_vec()),
        ("shop.json", br#"{"store": {"name": "shop"}}"#.to_vec()),
        ("sign.svg", SVG.as_bytes().to_vec()),
    ]
}

fn name_of(location: &Location) -> String {
    location
        .name()
        .map(|name| name.to_text())
        .unwrap_or_default()
}

fn listing() -> ArchiveListing {
    ArchiveListing {
        format: ArchiveFormat::Zip,
        entries: vec![ArchiveEntry {
            name: "docs/readme.txt".to_owned(),
            size: 24,
            packed: Some(12),
            modified: None,
            is_folder: false,
            is_encrypted: false,
        }],
        truncated: false,
    }
}

#[derive(Clone)]
pub(in crate::window_view) struct Disk {
    files: Arc<HashMap<String, Vec<u8>>>,
    open: Arc<AtomicUsize>,
}

impl Disk {
    pub(in crate::window_view) fn new(files: &[(&str, Vec<u8>)]) -> Self {
        let files = files
            .iter()
            .map(|(name, bytes)| ((*name).to_owned(), bytes.clone()))
            .collect();
        Self {
            files: Arc::new(files),
            open: Arc::default(),
        }
    }

    pub(in crate::window_view) fn open_handles(&self) -> usize {
        self.open.load(Ordering::SeqCst)
    }

    fn bytes_of(&self, location: &Location) -> Vec<u8> {
        self.files
            .get(&name_of(location))
            .cloned()
            .unwrap_or_default()
    }

    fn source(&self, location: &Location) -> OpenBytes {
        OpenBytes {
            bytes: self.bytes_of(location),
            _handle: Handle::open(&self.open),
        }
    }

    fn paged(&self, location: &Location) -> Paged {
        let (again, lines) = (self.clone(), self.bytes_of(location));
        let location = location.clone();
        Paged {
            source: Arc::new(self.source(&location)),
            reopen: Arc::new(move || {
                Ok(Box::new(again.source(&location)) as Box<dyn ByteSource + Send>)
            }),
            index: Arc::new(move |_, _| {
                let mut index = LineIndex::new();
                index.feed(&lines);
                index.finish();
                Ok(Some(index))
            }),
            summary: None,
            problem: None,
            editable: false,
            stamp: None,
        }
    }

    pub(in crate::window_view) fn hooks(&self) -> FileHooks {
        let (head, read, whole, paged, part, pdf) = (
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
            self.clone(),
        );
        FileHooks {
            head: Arc::new(move |location| {
                let bytes = head.bytes_of(location);
                Ok(Head {
                    len: bytes.len() as u64,
                    bytes: bytes.into_iter().take(8 * 1024).collect(),
                })
            }),
            read: Arc::new(move |location| {
                Ok(FileText::plain(String::from_utf8_lossy(
                    &read.bytes_of(location),
                )))
            }),
            bytes: Arc::new(move |location, _| Ok(whole.bytes_of(location))),
            paged: Arc::new(move |location| Ok(paged.paged(location))),
            range: Arc::new(move |location, offset, len| {
                let bytes = part.bytes_of(location);
                let start = usize::try_from(offset)
                    .unwrap_or(usize::MAX)
                    .min(bytes.len());
                Ok(bytes[start..(start + len).min(bytes.len())].to_vec())
            }),
            archive: Arc::new(|_, _| Ok(listing())),
            pdf: Arc::new(move |_, _| {
                Ok(Box::new(OpenPages {
                    _handle: Handle::open(&pdf.open),
                }) as Box<dyn PdfPages>)
            }),
            ..FileHooks::none()
        }
    }
}

fn cursor_name(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Option<String> {
    view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab();
        tab.listing()?
            .get(tab.view().cursor)
            .map(|entry| entry.name.clone())
    })
}

pub(in crate::window_view) fn cursor_to(
    cx: &mut TestAppContext,
    (window, view): (AnyWindowHandle, &Entity<FerriteWindow>),
    name: &str,
) {
    press(cx, window, view, "home");
    for _ in 0..64 {
        if cursor_name(cx, view).as_deref() == Some(name) {
            return;
        }
        press(cx, window, view, "down");
    }
    panic!("{name} is not in the list");
}

pub(in crate::window_view) fn held_in(cx: &mut TestAppContext, tally: &'static Tally) -> MemoryUse {
    cx.read(|cx| read_tally(tally, cx))
}

pub(in crate::window_view) fn gpu_images(cx: &mut TestAppContext) -> MemoryUse {
    held_in(cx, &GPU_IMAGES)
}

pub(in crate::window_view) fn assert_released(
    cx: &mut TestAppContext,
    name: &str,
    (body, disk): (&AnyWeakView, &Disk),
) {
    cx.run_until_parked();
    assert!(body.upgrade().is_none(), "{name}: the view is still alive");
    assert_eq!(disk.open_handles(), 0, "{name}: a file is still open");
    assert_eq!(gpu_images(cx).items, 0, "{name}: a GPU image is held");
}
