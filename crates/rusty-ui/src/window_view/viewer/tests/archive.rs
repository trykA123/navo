use std::sync::{Arc, Mutex};

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use rusty_fs::archive::{ArchiveEntry, ArchiveFormat, ArchiveListing, ENCRYPTED, Stamp};

use super::find::bottom_keys;
use super::{Log, Setup, hooks, label, press_only, redraw, text_of, visible, window_with};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{fake, press, settle, viewer_open};
use crate::window_view::viewer::archive::ArchiveBody;
use crate::window_view::viewer::{FileHooks, Kinds, PagedKind};

const ZIP_HEAD: &[u8] = b"PK\x03\x04\x14\x00\x00\x00\x08\x00";
const README: &[u8] = b"Ferrite reads archives.\n";

type Reads = Arc<Mutex<Vec<usize>>>;

fn entry(name: &str, size: u64) -> ArchiveEntry {
    ArchiveEntry {
        name: name.to_owned(),
        size,
        packed: Some(size / 2),
        modified: Some(Stamp::UnixMs(1_791_920_400_000)),
        is_folder: name.ends_with('/'),
        is_encrypted: false,
    }
}

fn listing() -> ArchiveListing {
    ArchiveListing {
        format: ArchiveFormat::Zip,
        entries: vec![
            entry("docs/", 0),
            entry("docs/readme.txt", README.len() as u64),
            entry("docs/data.bin", 6),
            ArchiveEntry {
                is_encrypted: true,
                ..entry("secret.txt", 9)
            },
        ],
        truncated: false,
    }
}

fn archive_hooks(listed: Result<ArchiveListing, String>, reads: &Reads) -> FileHooks {
    let contents = [Vec::new(), README.to_vec(), b"\x00\x01\x02bin".to_vec()];
    let reads = reads.clone();
    FileHooks {
        archive: Arc::new(move |_, _| listed.clone()),
        archive_entry: Arc::new(move |_, index, _| {
            reads.lock().expect("reads").push(index);
            contents
                .get(index)
                .cloned()
                .ok_or_else(|| "No such entry.".to_owned())
        }),
        ..hooks(Setup::new(ZIP_HEAD.to_vec()), &Log::default())
    }
}

fn open_zip(
    cx: &mut TestAppContext,
    listed: Result<ArchiveListing, String>,
) -> (AnyWindowHandle, Entity<FerriteWindow>, Reads) {
    let reads = Reads::default();
    let files = archive_hooks(listed, &reads);
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    (window, view, reads)
}

fn row(cx: &mut TestAppContext, window: AnyWindowHandle, index: usize) -> String {
    label(cx, window, ("row", index))
}

fn kind(cx: &mut TestAppContext, view: &Entity<FerriteWindow>) -> Option<PagedKind> {
    let archive = view.read_with(cx, |view, _| {
        let body = view.viewer.as_ref()?.body.as_ref()?;
        let archive = body.as_any().downcast_ref::<ArchiveBody>()?;
        Some(archive.0.clone())
    })?;
    archive.read_with(cx, |archive, cx| archive.nested_kind(cx))
}

fn reads(log: &Reads) -> Vec<usize> {
    log.lock().expect("reads").clone()
}

#[gpui_kit::test]
fn f3_on_a_zip_lists_its_entries_with_full_paths(cx: &mut TestAppContext) {
    let (window, _, reads_log) = open_zip(cx, Ok(listing()));
    assert_eq!(row(cx, window, 0), "docs, folder");
    assert!(row(cx, window, 1).starts_with("docs/readme.txt, 24 bytes, "));
    assert!(row(cx, window, 3).ends_with(", encrypted"));
    assert!(visible(cx, window, "archive-list"));
    let keys = bottom_keys(cx, window);
    assert!(keys.contains(&"Enter Open".to_owned()), "{keys:?}");
    assert!(keys.contains(&"1 Text".to_owned()), "{keys:?}");
    assert!(reads(&reads_log).is_empty());
}

#[gpui_kit::test]
fn enter_reads_the_entry_in_the_background_and_esc_returns(cx: &mut TestAppContext) {
    let (window, view, reads_log) = open_zip(cx, Ok(listing()));
    press(cx, window, &view, "down");
    press_only(cx, window, "enter");
    assert_eq!(
        text_of(cx, window, "archive-status"),
        "Reading docs/readme.txt…"
    );
    assert!(reads(&reads_log).is_empty());
    settle(cx, &view);
    redraw(cx, window);
    assert_eq!(reads(&reads_log), [1]);
    assert_eq!(kind(cx, &view), Some(PagedKind::Text));
    assert_eq!(
        label(cx, window, ("paged-row", 0)),
        "Ferrite reads archives."
    );
    press(cx, window, &view, "escape");
    assert!(viewer_open(cx, &view));
    assert!(visible(cx, window, "archive-list"));
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
}

#[gpui_kit::test]
fn typing_filters_and_a_binary_entry_opens_as_hex(cx: &mut TestAppContext) {
    let (window, view, reads_log) = open_zip(cx, Ok(listing()));
    for key in ["d", "a", "t", "a", "."] {
        press(cx, window, &view, key);
    }
    assert!(text_of(cx, window, "filter").starts_with("Filter data., 1 item"));
    assert!(row(cx, window, 0).starts_with("docs/data.bin, "));
    press(cx, window, &view, "enter");
    redraw(cx, window);
    assert_eq!(reads(&reads_log), [2]);
    assert_eq!(kind(cx, &view), Some(PagedKind::Hex));
    press(cx, window, &view, "escape");
    press(cx, window, &view, "escape");
    assert!(viewer_open(cx, &view));
    assert!(row(cx, window, 0).starts_with("docs, folder"));
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
}

#[gpui_kit::test]
fn filter_letters_that_are_lister_keys_still_type(cx: &mut TestAppContext) {
    let (window, view, _) = open_zip(cx, Ok(listing()));
    for key in ["s", "e", "c", "r", "e", "t", "5", "n", "p"] {
        press(cx, window, &view, key);
    }
    assert!(viewer_open(cx, &view));
    assert!(text_of(cx, window, "filter").starts_with("Filter secret5np, 0 items"));
    assert!(visible(cx, window, "archive-empty"));
    press(cx, window, &view, "backspace");
    press(cx, window, &view, "backspace");
    press(cx, window, &view, "backspace");
    press(cx, window, &view, "backspace");
    assert!(row(cx, window, 0).starts_with("secret.txt, "));
}

#[gpui_kit::test]
fn an_encrypted_entry_says_so_and_is_not_read(cx: &mut TestAppContext) {
    let (window, view, reads_log) = open_zip(cx, Ok(listing()));
    press(cx, window, &view, "end");
    press(cx, window, &view, "enter");
    assert_eq!(text_of(cx, window, "archive-status"), ENCRYPTED);
    assert!(reads(&reads_log).is_empty());
    press(cx, window, &view, "home");
    press(cx, window, &view, "enter");
    assert!(text_of(cx, window, "archive-status").starts_with("A folder"));
}

#[gpui_kit::test]
fn a_corrupt_archive_shows_its_message(cx: &mut TestAppContext) {
    let damaged = Err("This archive looks damaged: bad header".to_owned());
    let (window, view, _) = open_zip(cx, damaged);
    assert!(viewer_open(cx, &view));
    assert_eq!(
        text_of(cx, window, "viewer-message"),
        "This archive looks damaged: bad header"
    );
}

#[gpui_kit::test]
fn a_truncated_listing_says_how_many_entries_it_shows(cx: &mut TestAppContext) {
    let truncated = ArchiveListing {
        truncated: true,
        ..listing()
    };
    let (window, _, _) = open_zip(cx, Ok(truncated));
    assert_eq!(
        text_of(cx, window, "archive-status"),
        "Showing the first 4 entries."
    );
}
