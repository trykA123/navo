//! E7c: the F3 editor saves in another encoding or line ending chosen in the encoding picker.
//! The fake disk decodes and encodes like the app's file hooks, so each test checks real bytes.

use std::sync::{Arc, Mutex};

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use navo_core::encoding::{Decoded, decode, decode_as, encode};
use navo_core::line_ends::LineEnds;

use super::encoding::latin1;
use super::menu::run_item;
use super::{editor_text, redraw, text_of, window_with};
use crate::window_view::NavoWindow;
use crate::window_view::tests::{fake, press, settle, type_in};
use crate::window_view::viewer::{FileHooks, FileText, Head, Kinds, SaveError, Saved};

#[derive(Default)]
struct Disk {
    bytes: Vec<u8>,
    writes: usize,
}

type Shared = Arc<Mutex<Disk>>;

/// Decodes `bytes` the way the app's read hook does: detect, unify line endings, note CRLF.
fn read_back(bytes: &[u8]) -> FileText {
    let fallback = navo_core::encoding::Encoding::for_label(b"windows-1252").expect("label");
    file_text(decode(bytes, fallback))
}

/// Builds the editor's file from `decoded` text: unified line endings and the CRLF flag.
fn file_text(decoded: Decoded) -> FileText {
    let crlf = decoded.text.matches("\r\n").count() * 2 > decoded.text.matches('\n').count();
    let (text, ends) = match LineEnds::unify(&decoded.text) {
        Some((unified, ends)) => (unified, Some(Arc::new(ends))),
        None => (decoded.text, None),
    };
    FileText {
        crlf,
        read_only: !decoded.lossless,
        encoding: decoded.encoding,
        ends,
        ..FileText::plain(text)
    }
}

/// Builds the bytes of a save the way the app's write hook does, or the encode error text.
fn bytes_of(save: &crate::window_view::viewer::Save<'_>) -> Result<(Vec<u8>, Saved), SaveError> {
    let (text, ends) = match save.ends {
        Some(ends) => {
            let (text, ends) = ends.apply(save.text).expect("within the budget");
            (text, Some(ends))
        }
        None if save.crlf => (save.text.replace("\r\n", "\n").replace('\n', "\r\n"), None),
        None => (save.text.replace("\r\n", "\n"), None),
    };
    let bytes = encode(&text, &save.encoding).map_err(SaveError::Encode)?;
    Ok((bytes, Saved { stamp: None, ends }))
}

fn disk_hooks(disk: &Shared) -> FileHooks {
    let (head, read, write, read_as) = (disk.clone(), disk.clone(), disk.clone(), disk.clone());
    FileHooks {
        head: Arc::new(move |_| {
            let disk = head.lock().expect("disk");
            Ok(Head {
                bytes: disk.bytes.clone(),
                len: disk.bytes.len() as u64,
            })
        }),
        read: Arc::new(move |_| Ok(read_back(&read.lock().expect("disk").bytes))),
        read_as: Arc::new(move |_, encoding| {
            let disk = read_as.lock().expect("disk");
            Ok(file_text(decode_as(&disk.bytes, encoding)))
        }),
        write: Arc::new(move |_, save| {
            let (bytes, saved) = bytes_of(save)?;
            let mut disk = write.lock().expect("disk");
            disk.bytes = bytes;
            disk.writes += 1;
            Ok(saved)
        }),
        ..FileHooks::none()
    }
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<NavoWindow>,
    disk: Shared,
}

/// Opens `bytes` in F3 and starts editing.
fn edit(cx: &mut TestAppContext, bytes: Vec<u8>) -> Rig {
    let disk = Shared::new(Mutex::new(Disk {
        bytes,
        ..Disk::default()
    }));
    let (window, view) = window_with(cx, fake(), disk_hooks(&disk), Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    press(cx, window, &view, "ctrl-e");
    Rig { window, view, disk }
}

impl Rig {
    fn choose(&self, cx: &mut TestAppContext, text: &str) {
        press(cx, self.window, &self.view, "ctrl-shift-e");
        run_item(cx, self.window, text);
        settle(cx, &self.view);
        redraw(cx, self.window);
    }

    fn save(&self, cx: &mut TestAppContext) {
        press(cx, self.window, &self.view, "ctrl-s");
        settle(cx, &self.view);
        redraw(cx, self.window);
    }

    fn bytes(&self) -> Vec<u8> {
        self.disk.lock().expect("disk").bytes.clone()
    }

    fn writes(&self) -> usize {
        self.disk.lock().expect("disk").writes
    }

    fn is_unsaved(&self, cx: &mut TestAppContext) -> bool {
        self.view
            .read_with(cx, |view, cx| {
                view.viewer.as_ref().and_then(|viewer| viewer.unsaved(cx))
            })
            .is_some()
    }
}

fn utf16_le_bom(text: &str) -> Vec<u8> {
    let units = text.encode_utf16().flat_map(u16::to_le_bytes);
    [0xFF, 0xFE].into_iter().chain(units).collect()
}

#[gpui_kit::test]
fn utf8_saves_as_utf16_le_with_a_bom_and_reads_back_the_same_text(cx: &mut TestAppContext) {
    let text = "café\nnaïve ☕\n";
    let rig = edit(cx, text.as_bytes().to_vec());
    rig.choose(cx, "Save as UTF-16 LE BOM");
    rig.save(cx);
    assert_eq!(rig.bytes(), utf16_le_bom(text));
    assert_eq!(read_back(&rig.bytes()).text, text);
}

#[gpui_kit::test]
fn utf8_saves_as_windows_1252(cx: &mut TestAppContext) {
    let text = "Le café était prêt.\nÀ bientôt\n";
    let rig = edit(cx, text.as_bytes().to_vec());
    rig.choose(cx, "Save as Windows-1252");
    rig.save(cx);
    assert_eq!(rig.bytes(), latin1(text));
}

#[gpui_kit::test]
fn lf_saves_as_crlf(cx: &mut TestAppContext) {
    let rig = edit(cx, b"one\ntwo\nthree\n".to_vec());
    rig.choose(cx, "Windows line endings (CRLF)");
    rig.save(cx);
    assert_eq!(rig.bytes(), b"one\r\ntwo\r\nthree\r\n");
}

#[gpui_kit::test]
fn crlf_and_mixed_endings_save_as_lf(cx: &mut TestAppContext) {
    let rig = edit(cx, b"one\r\ntwo\nthree\r\n".to_vec());
    rig.choose(cx, "Unix line endings (LF)");
    rig.save(cx);
    assert_eq!(rig.bytes(), b"one\ntwo\nthree\n");
}

#[gpui_kit::test]
fn a_character_the_target_cannot_hold_refuses_the_save_and_leaves_the_file(
    cx: &mut TestAppContext,
) {
    let original = "price\n5 € or 6 Ω\n".as_bytes().to_vec();
    let rig = edit(cx, original.clone());
    rig.choose(cx, "Save as Windows-1252");
    rig.save(cx);
    assert_eq!(rig.bytes(), original);
    assert_eq!(rig.writes(), 0);
    let note = text_of(cx, rig.window, "viewer-message");
    assert!(note.contains("'Ω' on line 2"), "{note}");
}

#[gpui_kit::test]
fn a_save_without_a_choice_keeps_encoding_bom_and_endings_byte_for_byte(cx: &mut TestAppContext) {
    let original = utf16_le_bom("one\r\ntwo\nthree\r\n");
    let rig = edit(cx, original.clone());
    rig.save(cx);
    assert_eq!(rig.bytes(), original);
    press(cx, rig.window, &rig.view, "ctrl-end");
    type_in(cx, rig.window, &rig.view, "four");
    rig.save(cx);
    assert_eq!(rig.bytes(), utf16_le_bom("one\r\ntwo\nthree\r\nfour"));
}

#[gpui_kit::test]
fn a_conversion_is_unsaved_until_ctrl_s_and_then_becomes_the_loaded_encoding(
    cx: &mut TestAppContext,
) {
    let rig = edit(cx, b"hello\n".to_vec());
    rig.choose(cx, "Save as UTF-16 LE BOM");
    let dirty = rig.view.read_with(cx, |view, cx| {
        view.viewer.as_ref().and_then(|viewer| viewer.unsaved(cx))
    });
    assert!(dirty.is_some(), "a pending conversion counts as unsaved");
    rig.save(cx);
    type_in(cx, rig.window, &rig.view, "x");
    rig.save(cx);
    assert_eq!(rig.bytes(), utf16_le_bom("xhello\n"));
}

#[gpui_kit::test]
fn a_yen_sign_that_shift_jis_would_write_as_a_backslash_refuses_the_save(cx: &mut TestAppContext) {
    let original = "\u{A5}100\n".as_bytes().to_vec();
    let rig = edit(cx, original.clone());
    rig.choose(cx, "Save as Shift_JIS");
    rig.save(cx);
    assert_eq!(rig.writes(), 0);
    assert_eq!(rig.bytes(), original);
    let note = text_of(cx, rig.window, "viewer-message");
    let expected = navo_l10n::t!("viewer-save-unmappable", character = "\u{A5}", line = 1);
    assert_eq!(note, expected);
    assert!(note.contains("'\u{A5}' on line 1"), "{note}");
}

#[gpui_kit::test]
fn after_a_save_as_koi8_r_a_reload_reads_the_file_as_koi8_r(cx: &mut TestAppContext) {
    let text = "\u{41F}\u{440}\u{438}\u{432}\u{435}\u{442} \u{43C}\u{438}\u{440}\n";
    let rig = edit(cx, text.as_bytes().to_vec());
    rig.choose(cx, "Save as KOI8-R");
    rig.save(cx);
    assert_eq!(decode_as(&rig.bytes(), koi8_r()).text, text);
    let reads_as = rig
        .view
        .read_with(cx, |view, _| view.viewer.as_ref()?.encoding);
    assert_eq!(reads_as, Some(koi8_r()), "a reload reads through KOI8-R");
    press(cx, rig.window, &rig.view, "ctrl-shift-l");
    run_item(cx, rig.window, "Plain text");
    settle(cx, &rig.view);
    redraw(cx, rig.window);
    assert_eq!(editor_text(cx, &rig.view), text);
}

fn koi8_r() -> &'static navo_core::encoding::Encoding {
    navo_core::encoding::Encoding::for_label(b"KOI8-R").expect("label")
}

#[gpui_kit::test]
fn going_back_to_the_loaded_format_clears_the_pending_change(cx: &mut TestAppContext) {
    let rig = edit(cx, b"one\ntwo\n".to_vec());
    rig.choose(cx, "Unix line endings (LF)");
    assert!(rig.is_unsaved(cx));
    rig.choose(cx, "Line endings as loaded");
    assert!(
        !rig.is_unsaved(cx),
        "the as-loaded line endings are no change"
    );
    rig.choose(cx, "Save as Windows-1252");
    assert!(rig.is_unsaved(cx));
    rig.choose(cx, "Save as UTF-8");
    assert!(!rig.is_unsaved(cx), "the loaded encoding is no change");
    let note = text_of(cx, rig.window, "viewer-message");
    assert!(!note.contains("Ctrl+S saves it"), "{note}");
}

#[gpui_kit::test]
fn going_back_to_the_loaded_format_keeps_an_edit_unsaved(cx: &mut TestAppContext) {
    let rig = edit(cx, b"one\n".to_vec());
    type_in(cx, rig.window, &rig.view, "x");
    rig.choose(cx, "Unix line endings (LF)");
    rig.choose(cx, "Line endings as loaded");
    assert!(rig.is_unsaved(cx));
}
