//! Headless viewer tests for E14c: hex edit patch overlay, undo/redo, durable streaming save,
//! and file-stamp conflict refusal.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui_kit::TestAppContext;
use navo_core::paging::{ByteSource, Laid, TextLayout};
use navo_fs::{HEX_SAVE_CHUNK, HexFileStamp, PagedFile, save_patched_file};

use super::{Setup, label, paged, paged_window, redraw, text_of, visible, window_with};
use crate::window_view::tests::{fake, press, viewer_open};
use crate::window_view::viewer::{
    FileHooks, FileStamp, Head, HexSaveError, HexSaved, Kinds, Paged, PagedKind,
};

fn counting_binary(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 256) as u8).collect()
}

fn stamp_of(path: &Path) -> Option<FileStamp> {
    let meta = fs::metadata(path).ok()?;
    let modified_ms = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())?;
    Some(FileStamp {
        len: meta.len(),
        modified_ms,
    })
}

fn disk_hex_hooks(path: PathBuf) -> FileHooks {
    let head_path = path.clone();
    let paged_path = path.clone();
    let stamp_path = path.clone();
    let write_path = path;
    FileHooks {
        head: Arc::new(move |_| {
            let bytes = fs::read(&head_path).map_err(|e| e.to_string())?;
            let len = bytes.len() as u64;
            Ok(Head {
                bytes: bytes.into_iter().take(8 * 1024).collect(),
                len,
            })
        }),
        paged: Arc::new(move |_| {
            let raw = PagedFile::open(&paged_path)?;
            let layout = TextLayout::default();
            let reopen_path = paged_path.clone();
            Ok(Paged {
                source: Arc::new(Laid::new(raw, layout)),
                reopen: Arc::new(move || {
                    Ok(Box::new(Laid::new(PagedFile::open(&reopen_path)?, layout))
                        as Box<dyn ByteSource + Send>)
                }),
                index: Arc::new(|_, _| Ok(None)),
                summary: None,
                problem: None,
                editable: false,
                stamp: stamp_of(&paged_path),
            })
        }),
        hex_stamp: Arc::new(move |_| {
            HexFileStamp::from_path(&stamp_path)
                .map(Some)
                .map_err(|e| e.to_string())
        }),
        write_hex: Arc::new(move |_, save| {
            let expected = save
                .expected
                .ok_or_else(|| HexSaveError::Failed("missing stamp".to_owned()))?;
            let new_stamp = save_patched_file(&write_path, expected, save.patches)?;
            Ok(HexSaved {
                stamp: Some(new_stamp),
                file_stamp: stamp_of(&write_path),
            })
        }),
        ..FileHooks::none()
    }
}

#[gpui_kit::test]
fn hex_edit_overwrite_undo_redo_updates_rows_and_dirty_indicator(cx: &mut TestAppContext) {
    let (window, view) = paged_window(cx, Setup::new(counting_binary(4096)));
    assert_eq!(super::kind(cx, &view), Some(PagedKind::Hex));
    assert!(!visible(cx, window, "hex-edit-dirty"));

    press(cx, window, &view, "ctrl-e");
    press(cx, window, &view, "f");
    press(cx, window, &view, "f");
    redraw(cx, window);

    assert!(visible(cx, window, "hex-edit-dirty"));
    let row0 = label(cx, window, ("paged-row", 0));
    assert!(
        row0.contains("FF 01 02 03"),
        "expected patched byte FF at offset 0, got: {row0}"
    );
    let cursor = paged(cx, &view).read_with(cx, |view, _| view.cursor);
    assert_eq!(cursor, 1);

    // Undo restores original 0x00 and clears dirty state.
    press(cx, window, &view, "ctrl-z");
    redraw(cx, window);
    assert!(!visible(cx, window, "hex-edit-dirty"));
    let row0 = label(cx, window, ("paged-row", 0));
    assert!(
        row0.contains("00 01 02 03"),
        "expected undone byte 00 at offset 0, got: {row0}"
    );

    // Redo reapplies 0xFF and restores dirty state.
    press(cx, window, &view, "ctrl-y");
    redraw(cx, window);
    assert!(visible(cx, window, "hex-edit-dirty"));
    let row0 = label(cx, window, ("paged-row", 0));
    assert!(
        row0.contains("FF 01 02 03"),
        "expected redone byte FF at offset 0, got: {row0}"
    );
}

#[gpui_kit::test]
fn hex_edit_ctrl_s_streams_patches_to_disk_and_clears_dirty_state(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("image.bin");
    let total = HEX_SAVE_CHUNK + 2048;
    let mut expected = counting_binary(total);
    fs::write(&path, &expected).expect("write fixture");

    let (window, view) = window_with(cx, fake(), disk_hex_hooks(path.clone()), Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    assert_eq!(super::kind(cx, &view), Some(PagedKind::Hex));

    press(cx, window, &view, "ctrl-e");
    press(cx, window, &view, "a");
    press(cx, window, &view, "b");
    press(cx, window, &view, "c");
    press(cx, window, &view, "d");
    redraw(cx, window);
    assert!(visible(cx, window, "hex-edit-dirty"));

    press(cx, window, &view, "ctrl-s");
    redraw(cx, window);

    assert!(!visible(cx, window, "hex-edit-dirty"));
    expected[0] = 0xAB;
    expected[1] = 0xCD;
    assert_eq!(fs::read(&path).expect("read saved file"), expected);
    // Clean escape closes viewer immediately without unsaved warning.
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
}

#[gpui_kit::test]
fn hex_edit_ctrl_s_refuses_when_file_changed_on_disk(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("conflict.bin");
    let original = counting_binary(512);
    fs::write(&path, &original).expect("write original");

    let (window, view) = window_with(cx, fake(), disk_hex_hooks(path.clone()), Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);

    press(cx, window, &view, "ctrl-e");
    press(cx, window, &view, "d");
    press(cx, window, &view, "e");
    redraw(cx, window);

    // Externally modify file on disk after open.
    let newer = vec![0x99u8; 512];
    fs::write(&path, &newer).expect("external write");
    let later = SystemTime::now() + Duration::from_secs(120);
    File::options()
        .write(true)
        .open(&path)
        .expect("open for mtime")
        .set_modified(later)
        .expect("set mtime");

    press(cx, window, &view, "ctrl-s");
    redraw(cx, window);

    assert!(visible(cx, window, "hex-edit-dirty"));
    let status = text_of(cx, window, "paged-status");
    assert!(
        status.contains("Changed on disk") || status.contains("changed on disk"),
        "expected conflict notice in paged-status, got: {status}"
    );
    assert_eq!(fs::read(&path).expect("newer file untouched"), newer);
}
