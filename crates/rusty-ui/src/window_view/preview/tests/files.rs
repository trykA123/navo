use std::collections::HashMap;
use std::io::Cursor;
use std::sync::Arc;

use gpui_kit::AppContext as _;
use gpui_kit::TestAppContext;
use image::{ImageFormat, Rgba, RgbaImage};
use rusty_core::Location;
use rusty_core::marks::{Bookmarks, MarkSet};
use rusty_core::paging::{ByteSource, LineIndex, Needle};
use rusty_fs::FakeFs;
use rusty_fs::archive::{ArchiveEntry, ArchiveFormat, ArchiveListing};

use super::{Rig, open_on};
use rusty_core::encoding::TextEncoding;

use crate::window_view::preview::Shown;
use crate::window_view::preview::file::read_only;
use crate::window_view::{FileHooks, FileStamp, FileText, Head, Paged, Save};

const ARCHIVO: &[u8] = include_bytes!("../../../../fonts/Archivo-Regular.ttf");
const ZIP_HEAD: &[u8] = b"PK\x03\x04\x14\x00\x00\x00\x08\x00";

struct Bytes(Vec<u8>);

impl ByteSource for Bytes {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.0.as_slice().read_at(offset, buf)
    }
}

fn png() -> Vec<u8> {
    let picture = RgbaImage::from_pixel(6, 4, Rgba([40, 90, 160, 255]));
    let mut out = Cursor::new(Vec::new());
    picture.write_to(&mut out, ImageFormat::Png).unwrap();
    out.into_inner()
}

fn samples() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("blob.bin", (0..=255u8).collect()),
        ("bundle.zip", ZIP_HEAD.to_vec()),
        (
            "config.yaml",
            b"name: shop\nitems:\n  - first\n  - second\n".to_vec(),
        ),
        ("face.ttf", ARCHIVO.to_vec()),
        ("feed.xml", b"<feed><item>first</item></feed>".to_vec()),
        ("notes.txt", b"fn main() {}\nlet second = 2;\n".to_vec()),
        ("photo.png", png()),
        ("readme.md", b"# Ferrite\n\nA file manager.\n".to_vec()),
        (
            "sales.csv",
            b"date,units\n2026-07-01,3\n2026-07-02,5\n".to_vec(),
        ),
        ("shop.json", br#"{"store": {"name": "shop"}}"#.to_vec()),
    ]
}

fn name_of(location: &Location) -> String {
    location
        .name()
        .map(|name| name.to_text())
        .unwrap_or_default()
}

fn paged(bytes: Vec<u8>) -> Paged {
    let (source, again, lines) = (bytes.clone(), bytes.clone(), bytes);
    Paged {
        source: Arc::new(Bytes(source)),
        reopen: Arc::new(move || Ok(Box::new(Bytes(again.clone())) as Box<dyn ByteSource + Send>)),
        index: Arc::new(move |_, _| {
            let mut index = LineIndex::new();
            index.feed(&lines);
            index.finish();
            Ok(Some(index))
        }),
        summary: None,
        editable: false,
        stamp: None,
    }
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

fn disk_hooks(files: &Arc<HashMap<String, Vec<u8>>>) -> FileHooks {
    let bytes_of = {
        let files = files.clone();
        move |location: &Location| files.get(&name_of(location)).cloned().unwrap_or_default()
    };
    let (head, read, whole, pages, part) = (
        bytes_of.clone(),
        bytes_of.clone(),
        bytes_of.clone(),
        bytes_of.clone(),
        bytes_of,
    );
    FileHooks {
        head: Arc::new(move |location| {
            let bytes = head(location);
            Ok(Head {
                len: bytes.len() as u64,
                bytes: bytes.into_iter().take(8 * 1024).collect(),
            })
        }),
        read: Arc::new(move |location| {
            Ok(FileText::plain(String::from_utf8_lossy(&read(location))))
        }),
        bytes: Arc::new(move |location, _| Ok(whole(location))),
        paged: Arc::new(move |location| Ok(paged(pages(location)))),
        range: Arc::new(move |location, offset, len| {
            let bytes = part(location);
            let start = usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .min(bytes.len());
            Ok(bytes[start..(start + len).min(bytes.len())].to_vec())
        }),
        archive: Arc::new(|_, _| Ok(listing())),
        ..FileHooks::none()
    }
}

fn open_samples(cx: &mut TestAppContext) -> Rig {
    let fs = FakeFs::new();
    let mut stored = HashMap::new();
    for (name, bytes) in samples() {
        fs.add_file(&format!("home/{name}"), bytes.len() as u64);
        stored.insert(name.to_owned(), bytes);
    }
    let fs = Arc::new(fs);
    open_on(cx, &fs, fs.clone(), disk_hooks(&Arc::new(stored)))
}

fn shown_kind(cx: &mut TestAppContext, rig: &Rig) -> Option<(&'static str, bool)> {
    rig.read(cx, |view| match view.preview.shown()? {
        Shown::File(file) => Some((file.kind.as_ref()?.id(), file.body.is_some())),
        _ => None,
    })
}

#[gpui_kit::test]
fn every_file_kind_shows_its_body_read_only_in_the_sheet(cx: &mut TestAppContext) {
    let rig = open_samples(cx);
    rig.open(cx);
    let expected = [
        "hex", "archive", "tree", "font", "tree", "text", "image", "markdown", "table", "tree",
    ];
    for (index, kind) in expected.iter().enumerate() {
        if index > 0 {
            rig.key(cx, "down");
            rig.rest(cx);
        }
        let name = samples()[index].0;
        assert_eq!(rig.shown_name(cx).as_deref(), Some(name));
        assert_eq!(shown_kind(cx, &rig), Some((*kind, true)), "{name}");
        assert!(rig.shows(cx, "preview-body"), "{name} body");
        assert!(rig.list_has_focus(cx), "{name} focus");
    }
}

#[gpui_kit::test]
fn a_file_header_shows_the_name_the_size_and_the_kind_chip(cx: &mut TestAppContext) {
    let rig = open_samples(cx);
    rig.open(cx);
    for _ in 0..8 {
        rig.key(cx, "down");
    }
    rig.rest(cx);
    assert_eq!(rig.label(cx, "preview-name").as_deref(), Some("sales.csv"));
    assert_eq!(rig.label(cx, "preview-kind").as_deref(), Some("Table"));
    let size = crate::format::size(samples()[8].1.len() as u64);
    let meta = rig.label(cx, "preview-meta").unwrap_or_default();
    assert!(meta.starts_with(&size), "{meta}");
}

#[test]
fn the_preview_hooks_refuse_to_write() {
    let writes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = writes.clone();
    let (saved, remembered) = (writes.clone(), writes.clone());
    let hooks = FileHooks {
        write: Arc::new(move |_, _| {
            counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(None)
        }),
        save_memory: Arc::new(move |_, _, _, _| {
            saved.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }),
        remember: Arc::new(move |_| {
            remembered.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }),
        ..FileHooks::none()
    };
    let save = Save {
        text: "changed",
        crlf: false,
        expected: None,
        encoding: TextEncoding::default(),
    };
    let location = Location::new("/tmp/notes.txt").unwrap();
    let guarded = read_only(&hooks);
    assert!((guarded.write)(&location, &save).is_err());
    let stamp = FileStamp {
        len: 0,
        modified_ms: 0,
    };
    (guarded.save_memory)(&location, stamp, &MarkSet::default(), &Bookmarks::default());
    (guarded.remember)(&Needle::Bytes(vec![1]));
    assert_eq!(writes.load(std::sync::atomic::Ordering::Relaxed), 0);
}

fn body_has_focus(cx: &mut TestAppContext, rig: &Rig) -> bool {
    let view = rig.view.clone();
    cx.update_window(rig.window, |_, window, cx| {
        let view = view.read(cx);
        match view.preview.shown() {
            Some(Shown::File(file)) => file
                .body
                .as_ref()
                .is_some_and(|body| body.focus_handle(cx).is_focused(window)),
            _ => false,
        }
    })
    .expect("window")
}

#[gpui_kit::test]
fn browsing_a_file_preview_gives_focus_to_its_body(cx: &mut TestAppContext) {
    let rig = open_samples(cx);
    rig.open(cx);
    rig.key(cx, "down");
    rig.key(cx, "down");
    rig.rest(cx);
    assert_eq!(shown_kind(cx, &rig), Some(("tree", true)));
    rig.key(cx, "ctrl-shift-q");
    assert!(body_has_focus(cx, &rig));
    assert!(!rig.list_has_focus(cx));
    assert_eq!(
        rig.label(cx, "preview-browsing").as_deref(),
        Some("Browsing")
    );
    rig.key(cx, "down");
    assert!(body_has_focus(cx, &rig));
    rig.rest(cx);
    assert_eq!(rig.shown_name(cx).as_deref(), Some("config.yaml"));
    rig.key(cx, "escape");
    assert!(rig.list_has_focus(cx));
}
