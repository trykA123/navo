//! Cloud guard tests: quick view never reads a cloud placeholder (a OneDrive stub), whether the
//! bit comes from the live listing or from a fresh stat of a cached row.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use navo_core::{FsErrorKind, ProviderPath};
use navo_fs::{Attributes, EntryKind, FakeFs, FakeOp, Fault, FsEntry};

use super::{open_options, options_for, settle, two_panes};
use crate::window_view::preview::{Shown, may_read};
use crate::window_view::{FileHooks, FileText, Head, NavoWindow, StartOptions};

const STUB: &str = "home/stub.txt";

/// A file row named `stub.txt`, marked as a cloud placeholder when `cloud` is true.
fn stub(cloud: bool) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text("stub.txt"),
        kind: EntryKind::File,
        size: 4,
        modified_ms: None,
        attributes: Attributes {
            cloud_placeholder: cloud,
            ..Attributes::default()
        },
    }
}

/// File hooks that count every read of file bytes (head, text and raw bytes).
fn counting(reads: &Arc<AtomicUsize>) -> FileHooks {
    let (head, read, bytes) = (reads.clone(), reads.clone(), reads.clone());
    FileHooks {
        head: Arc::new(move |_| {
            head.fetch_add(1, Ordering::SeqCst);
            Ok(Head {
                bytes: b"data".to_vec(),
                len: 4,
            })
        }),
        read: Arc::new(move |_| {
            read.fetch_add(1, Ordering::SeqCst);
            Ok(FileText::plain("data"))
        }),
        bytes: Arc::new(move |_, _| {
            bytes.fetch_add(1, Ordering::SeqCst);
            Ok(b"data".to_vec())
        }),
        ..FileHooks::none()
    }
}

/// The message the file preview shows, once it has one.
fn shown_message(view: &NavoWindow) -> Option<String> {
    match view.preview.shown() {
        Some(Shown::File(file)) => file.message.as_ref().map(ToString::to_string),
        _ => None,
    }
}

/// Opens quick view on the cursor row and waits up to 10 s for its message.
fn quick_view_message(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
) -> Option<String> {
    cx.update_window(window, |_, window, cx| {
        view.update(cx, |view, cx| view.open_preview(window, cx));
    })
    .expect("window");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        cx.executor().advance_clock(Duration::from_secs(1));
        settle(cx, view);
        if let Some(message) = view.read_with(cx, |view, _| shown_message(view)) {
            return Some(message);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    None
}

#[gpui_kit::test]
fn cloud_placeholder_quick_view_reads_zero_bytes_and_says_open_to_download(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let fs = Arc::new(FakeFs::new());
    fs.add_entry("home", stub(true));
    let reads = Arc::new(AtomicUsize::new(0));
    let options = StartOptions {
        files: counting(&reads),
        ..options_for(fs.clone(), two_panes(&fs))
    };
    let (window, view) = open_options(cx, options);
    view.update(cx, |view, _| view.preview.set_every(Duration::ZERO));
    settle(cx, &view);
    let message = quick_view_message(cx, window, &view);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        0,
        "quick view read a placeholder"
    );
    assert_eq!(message, Some(navo_l10n::tr("preview-cloud-file")));
}

#[test]
fn cloud_bit_on_the_live_row_stops_the_read_without_a_stat() {
    let fs = FakeFs::new();
    fs.add_entry("home", stub(true));
    let result = may_read(&stub(true), &fs, &fs.location(STUB));
    assert_eq!(result, Err(navo_l10n::tr("preview-cloud-file")));
    assert_eq!(fs.calls(STUB, FakeOp::Metadata), 0);
}

#[test]
fn cloud_cached_row_without_the_bit_is_caught_by_a_fresh_stat() {
    let fs = FakeFs::new();
    fs.add_entry("home", stub(true));
    let result = may_read(&stub(false), &fs, &fs.location(STUB));
    assert_eq!(result, Err(navo_l10n::tr("preview-cloud-file")));
    assert_eq!(fs.calls(STUB, FakeOp::Metadata), 1);
}

#[test]
fn cloud_check_that_fails_stops_the_read() {
    let fs = FakeFs::new();
    fs.add_entry("home", stub(false));
    fs.fault(
        STUB,
        FakeOp::Metadata,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    assert!(may_read(&stub(false), &fs, &fs.location(STUB)).is_err());
}

#[test]
fn cloud_check_lets_a_plain_file_and_a_gone_file_through() {
    let fs = FakeFs::new();
    fs.add_entry("home", stub(false));
    assert_eq!(may_read(&stub(false), &fs, &fs.location(STUB)), Ok(()));
    let gone = fs.location("home/gone.txt");
    assert_eq!(may_read(&stub(false), &fs, &gone), Ok(()));
}
