//! A large text file shows the paged Text view on its first frame; the editor is read on a
//! background task and swapped in afterwards, keeping the scroll and a pending Ctrl+E (AU12).

use std::sync::Arc;

use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use navo_core::encoding::{Encoding, decode, detect};
use navo_core::paging::{ByteSource, Laid, LineIndex, TextLayout};

use super::{
    Setup, editor_of, has_editor, kind, paged, press_only, redraw, text_of, top, window_on,
    window_with,
};
use crate::window_view::NavoWindow;
use crate::window_view::tests::{fake, press, settle};
use crate::window_view::viewer::swap::SWAP_FROM;
use crate::window_view::viewer::{FileHooks, FileStamp, FileText, Head, Kinds, Paged, PagedKind};

/// Just over the swap size, so a test reads little more than the swap needs.
const LARGE: usize = SWAP_FROM as usize + 64 * 1024;
/// More frames than the swap needs: one to read, then layout, pin and show.
const SWAP_FRAMES: usize = 8;

/// Numbered short lines, `LARGE` bytes or a little more.
fn large_text() -> Vec<u8> {
    let mut text = Vec::with_capacity(LARGE + 32);
    let mut line = 0_u64;
    while text.len() < LARGE {
        text.extend_from_slice(format!("line {line}\n").as_bytes());
        line += 1;
    }
    text
}

/// The large text as UTF-16LE with a byte order mark: twice the bytes per character.
fn large_utf16() -> Vec<u8> {
    let text = String::from_utf8(large_text()).expect("ASCII");
    let units = text.encode_utf16().flat_map(u16::to_le_bytes);
    [0xFF, 0xFE].into_iter().chain(units).collect()
}

/// In-memory file bytes for the paged view.
struct Bytes(Arc<Vec<u8>>);

impl ByteSource for Bytes {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.0.as_slice().read_at(offset, buf)
    }
}

/// File hooks for `bytes` in their detected encoding, whose line index never runs by itself:
/// the test hands the index over with `finish_index`.
fn held_index_hooks(bytes: Vec<u8>) -> (FileHooks, TextLayout) {
    let fallback = Encoding::for_label(b"windows-1250").expect("a known label");
    let bytes = Arc::new(bytes);
    let layout = TextLayout::of(detect(&bytes, fallback));
    let (read, paged) = (bytes.clone(), bytes.clone());
    let head = Head {
        bytes: bytes.iter().take(8 * 1024).copied().collect(),
        len: bytes.len() as u64,
    };
    let stamp = Some(FileStamp {
        len: bytes.len() as u64,
        modified_ms: 1,
    });
    let hooks = FileHooks {
        head: Arc::new(move |_| Ok(head.clone())),
        read: Arc::new(move |_| {
            let decoded = decode(&read, fallback);
            Ok(FileText {
                text: decoded.text,
                encoding: decoded.encoding,
                stamp,
                ..FileText::plain("")
            })
        }),
        paged: Arc::new(move |_| {
            let (source, reopened) = (paged.clone(), paged.clone());
            Ok(Paged {
                source: Arc::new(Laid::new(Bytes(source), layout)),
                reopen: Arc::new(move || {
                    let laid = Laid::new(Bytes(reopened.clone()), layout);
                    Ok(Box::new(laid) as Box<dyn ByteSource + Send>)
                }),
                index: Arc::new(|_, _| Ok(None)),
                summary: None,
                problem: None,
                editable: false,
                stamp,
            })
        }),
        ..FileHooks::none()
    };
    (hooks, layout)
}

/// The 0-based row of byte `at` in UTF-16LE `bytes` with a byte order mark.
fn utf16_row(bytes: &[u8], at: u64) -> usize {
    let end = usize::try_from(at).expect("offset").min(bytes.len());
    let (units, _) = bytes[2..end].as_chunks::<2>();
    units.iter().filter(|unit| **unit == [b'\n', 0]).count()
}

/// Opens the large file with F3 and draws the first frame after the open finishes.
fn open_large(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<NavoWindow>, Vec<u8>) {
    let bytes = large_text();
    let (window, view, _) = window_on(cx, fake(), Setup::new(bytes.clone()));
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    settle(cx, &view);
    redraw(cx, window);
    (window, view, bytes)
}

/// Delivers the next frame, lets the background read finish and draws the result.
fn next_frame(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| window.simulate_next_frame(cx))
        .expect("window");
    cx.run_until_parked();
    redraw(cx, window);
}

/// Delivers frames until the editor has replaced the paged view; panics after `SWAP_FRAMES`.
pub(super) fn until_swapped(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
) {
    for _ in 0..SWAP_FRAMES {
        if has_editor(cx, view) {
            return;
        }
        next_frame(cx, window);
    }
    assert!(has_editor(cx, view), "the editor is swapped in");
}

#[gpui_kit::test]
fn a_large_text_file_draws_its_first_frame_before_the_editor_is_built(cx: &mut TestAppContext) {
    let (window, view, bytes) = open_large(cx);
    assert_eq!(
        kind(cx, &view),
        Some(PagedKind::Text),
        "the first frame shows the paged Text view"
    );
    assert!(!has_editor(cx, &view), "the editor is not built yet");
    until_swapped(cx, window, &view);
    let editor = editor_of(cx, &view).expect("the editor is swapped in");
    let len = editor.read_with(cx, |state, _| state.text().len());
    assert_eq!(len, bytes.len());
}

#[gpui_kit::test]
fn the_swap_keeps_the_top_line_of_the_paged_view(cx: &mut TestAppContext) {
    let (window, view, bytes) = open_large(cx);
    press(cx, window, &view, "pagedown");
    press(cx, window, &view, "pagedown");
    let at = usize::try_from(top(cx, &view)).expect("offset");
    assert!(at > 0, "the paged view scrolled");
    let row = bytes[..at].iter().filter(|byte| **byte == b'\n').count();
    for _ in 0..SWAP_FRAMES {
        next_frame(cx, window);
        let Some(editor) = editor_of(cx, &view) else {
            continue;
        };
        let rows = editor.read_with(cx, |state, _| state.visible_row_range());
        assert_eq!(
            rows.map(|rows| rows.start),
            Some(row),
            "the first frame that shows the editor has the paged view's top line"
        );
        return;
    }
    panic!("the editor is not swapped in after {SWAP_FRAMES} frames");
}

#[gpui_kit::test]
fn ctrl_e_before_the_swap_starts_editing_after_it(cx: &mut TestAppContext) {
    let (window, view, _) = open_large(cx);
    assert!(!has_editor(cx, &view), "Ctrl+E reaches the paged view");
    press(cx, window, &view, "ctrl-e");
    assert_eq!(text_of(cx, window, "viewer-message"), "");
    until_swapped(cx, window, &view);
    let editing = view.read_with(cx, |view, cx| {
        let body = view.viewer.as_ref().and_then(|viewer| viewer.body.as_ref());
        body.is_some_and(|body| body.is_editing(cx))
    });
    assert!(
        editing,
        "the Ctrl+E typed before the swap is applied after it"
    );
}

#[gpui_kit::test]
fn a_utf16_swap_after_ctrl_end_waits_for_the_paged_top_line(cx: &mut TestAppContext) {
    let bytes = large_utf16();
    let (hooks, layout) = held_index_hooks(bytes.clone());
    let (window, view) = window_with(cx, fake(), hooks, Kinds::builtin());
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    settle(cx, &view);
    redraw(cx, window);
    assert_eq!(kind(cx, &view), Some(PagedKind::Text));
    press(cx, window, &view, "ctrl-end");
    let at = top(cx, &view);
    assert!(
        at > SWAP_FROM / 2,
        "Ctrl+End moved the paged view to the end"
    );
    let row = utf16_row(&bytes, at);
    for frame in 0..2 * SWAP_FRAMES {
        if frame == 2 {
            let mut index = LineIndex::with_layout(layout);
            index.feed(&bytes);
            index.finish();
            let paged = paged(cx, &view);
            paged.update(cx, |paged, cx| paged.finish_index(index, cx));
        }
        next_frame(cx, window);
        let Some(editor) = editor_of(cx, &view) else {
            continue;
        };
        let rows = editor.read_with(cx, |state, _| state.visible_row_range());
        assert_eq!(
            rows.map(|rows| rows.start),
            Some(row),
            "the first frame that shows the editor has the paged view's top line"
        );
        return;
    }
    panic!(
        "the editor is not swapped in after {} frames",
        2 * SWAP_FRAMES
    );
}

#[gpui_kit::test]
fn a_late_pin_from_the_previous_file_does_not_show_the_next_files_editor(cx: &mut TestAppContext) {
    let fs = fake();
    fs.add_file("home/b.txt", 3);
    let bytes = large_text();
    let (window, view, _) = window_on(cx, fs, Setup::new(bytes.clone()));
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    settle(cx, &view);
    redraw(cx, window);
    next_frame(cx, window);
    let title = |cx: &mut TestAppContext| {
        view.read_with(cx, |view, _| {
            view.viewer.as_ref().map(|viewer| viewer.title.to_string())
        })
    };
    assert_eq!(title(cx).as_deref(), Some("a.txt"));
    assert!(
        !has_editor(cx, &view),
        "a.txt's editor is still being pinned"
    );
    press(cx, window, &view, "n");
    settle(cx, &view);
    assert_eq!(title(cx).as_deref(), Some("b.txt"));
    press(cx, window, &view, "pagedown");
    let at = usize::try_from(top(cx, &view)).expect("offset");
    assert!(at > 0, "b.txt's paged view scrolled");
    let row = bytes[..at].iter().filter(|byte| **byte == b'\n').count();
    let staged = |cx: &mut TestAppContext| {
        view.read_with(cx, |view, _| {
            view.viewer
                .as_ref()
                .is_some_and(|viewer| viewer.staged_view().is_some())
        })
    };
    let mut staged_at = None;
    for frame in 0..SWAP_FRAMES {
        next_frame(cx, window);
        let Some(editor) = editor_of(cx, &view) else {
            staged_at = staged_at.or(staged(cx).then_some(frame));
            continue;
        };
        let staged_at = staged_at.expect("b.txt's editor was staged first");
        assert!(
            frame >= staged_at + 3,
            "b.txt's editor is shown only after its own layout, pin and show frames"
        );
        let rows = editor.read_with(cx, |state, _| state.visible_row_range());
        assert_eq!(rows.map(|rows| rows.start), Some(row));
        return;
    }
    panic!("b.txt's editor is not swapped in after {SWAP_FRAMES} frames");
}

#[gpui_kit::test]
fn a_large_text_file_disables_soft_wrap_to_avoid_full_file_shaping(cx: &mut TestAppContext) {
    let (window, view, _) = open_large(cx);
    until_swapped(cx, window, &view);
    let wraps = view.read_with(cx, |view, cx| {
        let body = view
            .viewer
            .as_ref()
            .and_then(|viewer| viewer.body.as_ref())
            .expect("viewer body");
        let editor = body
            .as_any()
            .downcast_ref::<crate::window_view::viewer::editor::EditorBody>()
            .expect("editor body");
        editor.0.read(cx).wraps()
    });
    assert!(
        !wraps,
        "large files must disable soft wrap so gpui does not shape every line eagerly"
    );
}

#[gpui_kit::test]
fn closing_f3_drops_the_viewer_editor_and_paged_entities(cx: &mut TestAppContext) {
    let (window, view, _) = open_large(cx);
    let weak_paged = paged(cx, &view).downgrade();
    until_swapped(cx, window, &view);
    let (weak_editor_view, weak_editor_state) = view.read_with(cx, |view, cx| {
        let body = view
            .viewer
            .as_ref()
            .and_then(|viewer| viewer.body.as_ref())
            .expect("viewer body");
        let editor = body
            .as_any()
            .downcast_ref::<crate::window_view::viewer::editor::EditorBody>()
            .expect("editor body");
        (editor.0.downgrade(), editor.editor(cx).downgrade())
    });
    press(cx, window, &view, "escape");
    redraw(cx, window);
    redraw(cx, window);
    cx.run_until_parked();
    assert!(
        weak_paged.upgrade().is_none(),
        "PagedView must be released after swap and viewer close"
    );
    assert!(
        weak_editor_view.upgrade().is_none(),
        "EditorView must be released when F3 closes"
    );
    assert!(
        weak_editor_state.upgrade().is_none(),
        "EditorState must be released when F3 closes"
    );
}
