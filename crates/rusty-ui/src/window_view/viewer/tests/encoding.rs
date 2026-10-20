use std::sync::{Arc, Mutex};

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use rusty_core::encoding::{
    Decoded, Encoding, TextEncoding, choose, decode, decode_as, detect, is_lossy,
};
use rusty_core::paging::{ByteSource, Laid, LineIndex, TextLayout};

use super::menu::{hint, press_key, run_item};
use super::{label, redraw, text_of, visible, window_with};
use crate::window_view::FerriteWindow;
use crate::window_view::tests::{fake, press, settle, type_in};
use crate::window_view::viewer::{
    EDIT_LIMIT, FileHooks, FileStamp, FileText, Head, Kinds, Paged, Saved,
};

fn fallback() -> &'static Encoding {
    Encoding::for_label(b"windows-1250").expect("a known label")
}

const FRENCH: &str =
    "Le café était déjà prêt à côté de l'hôpital, où la clé du théâtre restait oubliée.\n";

struct Bytes(Arc<Vec<u8>>);

impl ByteSource for Bytes {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.0.as_slice().read_at(offset, buf)
    }
}

fn latin1(text: &str) -> Vec<u8> {
    text.chars()
        .map(|character| u8::try_from(u32::from(character)).unwrap())
        .collect()
}

fn utf16_le_with_bom(text: &str) -> Vec<u8> {
    let units = text.encode_utf16().flat_map(u16::to_le_bytes);
    [0xFF, 0xFE].into_iter().chain(units).collect()
}

#[derive(Clone, Default)]
struct Calls(Arc<Mutex<Vec<String>>>);

impl Calls {
    fn add(&self, call: String) {
        self.0.lock().expect("calls").push(call);
    }

    fn all(&self) -> Vec<String> {
        self.0.lock().expect("calls").clone()
    }
}

fn file_of(decoded: Decoded) -> FileText {
    FileText {
        text: decoded.text,
        read_only: !decoded.lossless,
        encoding: decoded.encoding,
        stamp: Some(FileStamp {
            len: 1,
            modified_ms: 1,
        }),
        ..FileText::plain("")
    }
}

fn paged_laid(bytes: &Arc<Vec<u8>>, encoding: TextEncoding) -> Paged {
    let layout = TextLayout::of(encoding);
    let (reopened, indexed) = (bytes.clone(), bytes.clone());
    let head = &bytes[..bytes.len().min(64 * 1024)];
    Paged {
        source: Arc::new(Laid::new(Bytes(bytes.clone()), layout)),
        reopen: Arc::new(move || {
            Ok(Box::new(Laid::new(Bytes(reopened.clone()), layout)) as Box<dyn ByteSource + Send>)
        }),
        index: Arc::new(move |_, progress| {
            let mut index = LineIndex::with_layout(layout);
            index.feed(&indexed);
            index.finish();
            progress(&index);
            Ok(Some(index))
        }),
        summary: None,
        problem: is_lossy(head, &encoding)
            .then(|| format!("Some bytes are not valid as {}.", encoding.label())),
        editable: false,
        stamp: Some(FileStamp {
            len: bytes.len() as u64,
            modified_ms: 1,
        }),
    }
}

fn hooks_for(bytes: Vec<u8>, len: u64, calls: &Calls) -> FileHooks {
    let bytes = Arc::new(bytes);
    let head = Head {
        bytes: bytes.iter().take(8 * 1024).copied().collect(),
        len,
    };
    let (read_log, read_as_log, paged_log, paged_as_log) =
        (calls.clone(), calls.clone(), calls.clone(), calls.clone());
    let (a, b, c, d) = (bytes.clone(), bytes.clone(), bytes.clone(), bytes);
    FileHooks {
        head: Arc::new(move |_| Ok(head.clone())),
        read: Arc::new(move |_| {
            read_log.add("read".to_owned());
            Ok(file_of(decode(&a, fallback())))
        }),
        read_as: Arc::new(move |_, encoding: &'static Encoding| {
            read_as_log.add(format!("read_as {}", encoding.name()));
            Ok(file_of(decode_as(&b, encoding)))
        }),
        paged: Arc::new(move |_| {
            paged_log.add("paged".to_owned());
            Ok(paged_laid(&c, detect(&c, fallback())))
        }),
        paged_as: Arc::new(move |_, encoding: &'static Encoding| {
            paged_as_log.add(format!("paged_as {}", encoding.name()));
            Ok(paged_laid(&d, choose(&d, encoding)))
        }),
        write: Arc::new(|_, _| Ok(None.into())),
        ..FileHooks::none()
    }
}

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    calls: Calls,
}

fn open(cx: &mut TestAppContext, bytes: Vec<u8>, len: Option<u64>) -> Rig {
    let calls = Calls::default();
    let size = len.unwrap_or(bytes.len() as u64);
    let files = hooks_for(bytes, size, &calls);
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    Rig {
        window,
        view,
        calls,
    }
}

impl Rig {
    fn press(&self, cx: &mut TestAppContext, key: &str) {
        press(cx, self.window, &self.view, key);
    }

    fn encoding(&self, cx: &mut TestAppContext) -> Option<TextEncoding> {
        self.view.read_with(cx, |view, cx| {
            view.viewer.as_ref()?.body.as_ref()?.encoding(cx)
        })
    }

    fn label(&self, cx: &mut TestAppContext) -> String {
        self.encoding(cx)
            .map(|found| found.label())
            .unwrap_or_default()
    }

    fn rows(&self, cx: &mut TestAppContext) -> Vec<(String, bool)> {
        self.view
            .read_with(cx, |view, cx| {
                let menu = view.viewer_menu_view()?;
                let items = menu.read(cx).menu().items.clone();
                Some(
                    items
                        .into_iter()
                        .map(|item| (item.text, item.checked))
                        .collect(),
                )
            })
            .unwrap_or_default()
    }

    fn choose(&self, cx: &mut TestAppContext, text: &str) {
        self.press(cx, "ctrl-shift-e");
        run_item(cx, self.window, text);
        settle(cx, &self.view);
        redraw(cx, self.window);
    }

    fn editor_text(&self, cx: &mut TestAppContext) -> String {
        super::editor_text(cx, &self.view)
    }
}

fn names(rows: &[(String, bool)]) -> Vec<&str> {
    rows.iter().map(|(text, _)| text.as_str()).collect()
}

#[gpui_kit::test]
fn ctrl_shift_e_lists_auto_and_the_encodings_with_the_current_one_checked(cx: &mut TestAppContext) {
    let rig = open(cx, latin1(FRENCH), None);
    assert_eq!(rig.label(cx), "Windows-1252");
    rig.press(cx, "ctrl-shift-e");
    let rows = rig.rows(cx);
    assert_eq!(rows[0], ("Auto (detected: Windows-1252)".to_owned(), true));
    for wanted in [
        "UTF-8",
        "UTF-16 LE",
        "UTF-16 BE",
        "Windows-1250",
        "Windows-1251",
        "Windows-1252 \u{b7} ISO-8859-1",
        "ISO-8859-2",
    ] {
        assert!(names(&rows).contains(&wanted), "{wanted} is offered");
    }
    assert!(rows.iter().skip(1).all(|(_, checked)| !checked));
    press_key(cx, rig.window, "escape");
    assert!(rig.rows(cx).is_empty());
}

#[gpui_kit::test]
fn choosing_an_encoding_rereads_the_file_through_the_chosen_one(cx: &mut TestAppContext) {
    let rig = open(cx, latin1(FRENCH), None);
    assert_eq!(rig.editor_text(cx), FRENCH);
    rig.choose(cx, "Windows-1251");
    assert_eq!(rig.calls.all(), ["read", "read_as windows-1251"]);
    assert_eq!(rig.label(cx), "Windows-1251");
    assert!(rig.editor_text(cx).starts_with("Le "));
    assert_ne!(rig.editor_text(cx), FRENCH);
    rig.press(cx, "ctrl-shift-e");
    let rows = rig.rows(cx);
    assert!(rows.contains(&("Windows-1251".to_owned(), true)));
    assert!(rows.contains(&("Auto (detected: Windows-1252)".to_owned(), false)));
}

#[gpui_kit::test]
fn auto_returns_to_the_detected_encoding(cx: &mut TestAppContext) {
    let rig = open(cx, latin1(FRENCH), None);
    rig.choose(cx, "UTF-16 LE");
    assert_eq!(rig.label(cx), "UTF-16 LE");
    rig.choose(cx, "Auto");
    assert_eq!(rig.calls.all(), ["read", "read_as UTF-16LE", "read"]);
    assert_eq!(rig.label(cx), "Windows-1252");
    assert_eq!(rig.editor_text(cx), FRENCH);
}

#[gpui_kit::test]
fn a_lossy_decode_is_shown_as_a_problem_and_stays_read_only(cx: &mut TestAppContext) {
    let rig = open(cx, latin1(FRENCH), None);
    assert!(!visible(cx, rig.window, "viewer-message"));
    rig.choose(cx, "UTF-8");
    assert!(rig.editor_text(cx).contains('\u{FFFD}'));
    let message = text_of(cx, rig.window, "viewer-message");
    assert!(message.contains("Not valid as UTF-8"), "{message}");
    rig.press(cx, "ctrl-e");
    let editing = rig.view.read_with(cx, |view, cx| {
        view.viewer
            .as_ref()
            .and_then(|v| v.body.as_ref().map(|b| b.is_editing(cx)))
    });
    assert_eq!(editing, Some(false));
}

#[gpui_kit::test]
fn the_view_menu_has_an_encoding_row_that_opens_the_picker(cx: &mut TestAppContext) {
    let rig = open(cx, latin1(FRENCH), None);
    rig.press(cx, "alt-v");
    assert!(names(&rig.rows(cx)).contains(&"Encoding\u{2026}"));
    assert_eq!(
        hint(cx, &rig.view, "Encoding\u{2026}").as_deref(),
        Some("Ctrl+Shift+E")
    );
    run_item(cx, rig.window, "Encoding\u{2026}");
    assert_eq!(rig.rows(cx)[0].0, "Auto (detected: Windows-1252)");
}

#[gpui_kit::test]
fn unsaved_edits_survive_until_the_choice_is_repeated(cx: &mut TestAppContext) {
    let rig = open(cx, latin1(FRENCH), None);
    rig.press(cx, "ctrl-e");
    type_in(cx, rig.window, &rig.view, "x");
    rig.choose(cx, "UTF-8");
    assert_eq!(rig.calls.all(), ["read"]);
    assert!(rig.editor_text(cx).starts_with('x'));
    assert!(text_of(cx, rig.window, "viewer-message").contains("Unsaved changes"));
    rig.choose(cx, "UTF-8");
    assert_eq!(rig.calls.all(), ["read", "read_as UTF-8"]);
}

#[gpui_kit::test]
fn a_big_utf16_file_opens_as_paged_text_with_decoded_rows(cx: &mut TestAppContext) {
    let text = "alpha wörld 日本語\r\nbeta\r\ngamma\r\n".repeat(50);
    let rig = open(cx, utf16_le_with_bom(&text), Some(EDIT_LIMIT + 1));
    assert_eq!(rig.calls.all(), ["paged"]);
    assert_eq!(rig.label(cx), "UTF-16 LE BOM");
    assert_eq!(
        label(cx, rig.window, ("paged-row", 0)),
        "alpha wörld 日本語"
    );
    assert_eq!(label(cx, rig.window, ("paged-row", 2)), "gamma");
}

#[gpui_kit::test]
fn a_big_windows_1252_file_opens_as_paged_text_with_decoded_rows(cx: &mut TestAppContext) {
    let rig = open(cx, latin1(&FRENCH.repeat(40)), Some(EDIT_LIMIT + 1));
    assert_eq!(rig.label(cx), "Windows-1252");
    assert_eq!(label(cx, rig.window, ("paged-row", 0)), FRENCH.trim_end());
}

#[gpui_kit::test]
fn choosing_an_encoding_in_a_big_file_rereads_the_rows_and_reports_a_lossy_head(
    cx: &mut TestAppContext,
) {
    let rig = open(cx, latin1(&FRENCH.repeat(40)), Some(EDIT_LIMIT + 1));
    assert!(!visible(cx, rig.window, "paged-problem"));
    rig.choose(cx, "UTF-8");
    assert_eq!(rig.calls.all(), ["paged", "paged_as UTF-8"]);
    assert_eq!(rig.label(cx), "UTF-8");
    assert!(label(cx, rig.window, ("paged-row", 0)).contains('\u{FFFD}'));
    assert!(visible(cx, rig.window, "paged-problem"));
    rig.choose(cx, "Auto");
    assert_eq!(label(cx, rig.window, ("paged-row", 0)), FRENCH.trim_end());
    assert!(!visible(cx, rig.window, "paged-problem"));
}

#[gpui_kit::test]
fn the_hex_view_has_no_encoding_to_choose(cx: &mut TestAppContext) {
    let rig = open(cx, latin1(&FRENCH.repeat(40)), Some(EDIT_LIMIT + 1));
    rig.press(cx, "alt-3");
    assert_eq!(rig.encoding(cx), None);
    rig.press(cx, "ctrl-shift-e");
    assert!(rig.rows(cx).is_empty());
    assert!(text_of(cx, rig.window, "paged-status").contains("text view"));
}

#[gpui_kit::test]
fn a_big_shift_jis_file_chosen_in_the_picker_opens_as_hex_with_a_note(cx: &mut TestAppContext) {
    let rig = open(cx, latin1(&FRENCH.repeat(40)), Some(EDIT_LIMIT + 1));
    rig.choose(cx, "Shift_JIS");
    assert_eq!(rig.calls.all(), ["paged", "paged_as Shift_JIS"]);
    assert_eq!(rig.encoding(cx), None);
    let note = text_of(cx, rig.window, "paged-status");
    assert!(note.contains("needs the whole file"), "{note}");
}

fn mixed_hooks(original: &'static str, written: &Arc<Mutex<Vec<String>>>) -> FileHooks {
    let (unified, ends) = rusty_core::line_ends::LineEnds::unify(original).expect("mixed");
    let ends = Arc::new(ends);
    let log = written.clone();
    FileHooks {
        head: Arc::new(move |_| {
            Ok(Head {
                bytes: original.as_bytes().to_vec(),
                len: original.len() as u64,
            })
        }),
        read: Arc::new(move |_| {
            Ok(FileText {
                ends: Some(ends.clone()),
                ..FileText::plain(unified.clone())
            })
        }),
        write: Arc::new(move |_, save| {
            let (bytes, ends) = match save.ends {
                Some(ends) => ends
                    .apply(save.text)
                    .map(|(bytes, ends)| (bytes, Some(ends)))
                    .expect("within the budget"),
                None => (save.text.to_owned(), None),
            };
            log.lock().expect("log").push(bytes);
            Ok(Saved { stamp: None, ends })
        }),
        ..FileHooks::none()
    }
}

#[gpui_kit::test]
fn a_file_opened_read_only_for_a_lone_cr_says_so(cx: &mut TestAppContext) {
    let original = "a\r\r\nb\nc";
    let (unified, ends) = rusty_core::line_ends::LineEnds::unify(original).expect("mixed");
    let ends = Arc::new(ends);
    let files = FileHooks {
        head: Arc::new(move |_| {
            Ok(Head {
                bytes: original.as_bytes().to_vec(),
                len: original.len() as u64,
            })
        }),
        read: Arc::new(move |_| {
            Ok(FileText {
                read_only: true,
                ends: Some(ends.clone()),
                ..FileText::plain(unified.clone())
            })
        }),
        ..FileHooks::none()
    };
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    let message = text_of(cx, window, "viewer-message");
    assert_eq!(
        message,
        "Opened read-only: a line ends with a lone CR, so its line endings cannot be kept exactly."
    );
}

fn last_written(written: &Arc<Mutex<Vec<String>>>) -> Option<String> {
    written.lock().expect("log").last().cloned()
}

#[gpui_kit::test]
fn a_mixed_line_ending_file_is_editable_and_saves_each_line_with_its_own_ending(
    cx: &mut TestAppContext,
) {
    let original = "one\r\ntwo\nthree\r\nfour\n";
    let written: Arc<Mutex<Vec<String>>> = Arc::default();
    let files = mixed_hooks(original, &written);
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    press(cx, window, &view, "ctrl-e");
    press(cx, window, &view, "ctrl-s");
    assert_eq!(last_written(&written).as_deref(), Some(original));
    press(cx, window, &view, "ctrl-home");
    type_in(cx, window, &view, "X");
    press(cx, window, &view, "ctrl-s");
    assert_eq!(
        last_written(&written).as_deref(),
        Some("Xone\r\ntwo\nthree\r\nfour\n")
    );
}

#[gpui_kit::test]
fn a_second_save_keeps_the_endings_the_first_save_wrote(cx: &mut TestAppContext) {
    let original = "one\r\ntwo\nthree\r\nfour\n";
    let written: Arc<Mutex<Vec<String>>> = Arc::default();
    let (window, view) = window_with(
        cx,
        fake(),
        mixed_hooks(original, &written),
        Kinds::builtin(),
    );
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    press(cx, window, &view, "ctrl-e");
    press(cx, window, &view, "ctrl-home");
    type_in(cx, window, &view, "new\n");
    press(cx, window, &view, "ctrl-s");
    assert_eq!(
        last_written(&written).as_deref(),
        Some("new\none\r\ntwo\nthree\r\nfour\n")
    );
    press(cx, window, &view, "down");
    press(cx, window, &view, "down");
    type_in(cx, window, &view, "X");
    press(cx, window, &view, "ctrl-s");
    assert_eq!(
        last_written(&written).as_deref(),
        Some("new\none\r\ntwo\nXthree\r\nfour\n")
    );
}
