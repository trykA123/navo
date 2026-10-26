use std::sync::{Arc, Mutex};

use gpui_kit::TestAppContext;

use super::{Log, Setup, hooks, label, press_only, redraw, text_of, visible, window_with};
use crate::window_view::tests::{fake, press, settle, viewer_open};
use crate::window_view::viewer::font::{SAMPLE, SIZES};
use crate::window_view::viewer::{FileHooks, Kinds};

const INTER: &[u8] = include_bytes!("../../../../fonts/Inter-Regular.ttf");

type Limits = Arc<Mutex<Vec<u64>>>;

fn font_hooks(bytes: &'static [u8], limits: &Limits) -> FileHooks {
    let limits = limits.clone();
    FileHooks {
        bytes: Arc::new(move |_, limit| {
            limits.lock().expect("limits").push(limit);
            Ok(bytes.to_vec())
        }),
        ..hooks(Setup::new(bytes.to_vec()), &Log::default())
    }
}

#[gpui_kit::test]
fn a_font_shows_its_family_and_sample_lines_after_a_background_read(cx: &mut TestAppContext) {
    let limits = Limits::default();
    let files = font_hooks(INTER, &limits);
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    assert!(limits.lock().expect("limits").is_empty());
    settle(cx, &view);
    redraw(cx, window);
    assert_eq!(*limits.lock().expect("limits"), [32 * 1024 * 1024]);
    assert_eq!(text_of(cx, window, "font-family"), "Inter");
    for index in 0..SIZES.len() {
        assert_eq!(label(cx, window, ("font-sample", index)), SAMPLE);
    }
    assert!(text_of(cx, window, "font-glyphs").ends_with(" characters"));
    press(cx, window, &view, "pagedown");
    press(cx, window, &view, "escape");
    assert!(!viewer_open(cx, &view));
}

#[gpui_kit::test]
fn a_damaged_font_shows_a_message(cx: &mut TestAppContext) {
    let limits = Limits::default();
    let mut broken = INTER[..64].to_vec();
    broken.extend_from_slice(&[0; 64]);
    let broken: &'static [u8] = Box::leak(broken.into_boxed_slice());
    let files = font_hooks(broken, &limits);
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    assert!(text_of(cx, window, "viewer-message").starts_with("This font looks damaged"));
}

fn be(bytes: &[u8], at: usize, width: usize) -> usize {
    bytes[at..at + width]
        .iter()
        .fold(0, |value, byte| (value << 8) | usize::from(*byte))
}

fn with_unreadable_last_name() -> &'static [u8] {
    let mut font = INTER.to_vec();
    let tables = be(&font, 4, 2);
    let name = (0..tables)
        .map(|index| 12 + 16 * index)
        .find(|record| &font[*record..*record + 4] == b"name")
        .map(|record| be(&font, record + 8, 4))
        .expect("a name table");
    let last = name + 6 + 12 * (be(&font, name + 2, 2) - 1);
    font[last + 8..last + 10].copy_from_slice(&u16::MAX.to_be_bytes());
    Box::leak(font.into_boxed_slice())
}

#[gpui_kit::test]
fn a_font_whose_names_cannot_be_rewritten_shows_facts_without_a_preview(cx: &mut TestAppContext) {
    let limits = Limits::default();
    let files = font_hooks(with_unreadable_last_name(), &limits);
    let (window, view) = window_with(cx, fake(), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    assert_eq!(text_of(cx, window, "font-family"), "Inter");
    assert_eq!(
        text_of(cx, window, "font-note"),
        "Preview not available for this font."
    );
    assert!(!visible(cx, window, "font-glyphs"));
}
