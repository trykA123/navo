//! E5: lint diagnostics in the F3 editor — debounced background lint, F8 jumps, header count.
//! Drives the real window with keys and the test clock; reads the widget's diagnostic set.

use std::ops::Range;
use std::time::Duration;

use gpui_kit::base::input::DiagnosticSeverity;
use gpui_kit::component::theme::Theme as KitTheme;
use gpui_kit::{AnyWindowHandle, Entity, Hsla, Rgba, TestAppContext};
use navo_structured::lint::{LintKind, lint};

use super::structured::open_named;
use super::{Setup, editor_of, editor_text, press_only, redraw, text_of};
use crate::tokens::{self, Mode};
use crate::window_view::NavoWindow;
use crate::window_view::tests::{press, settle};

const BROKEN: &[u8] = br#"{"a":1,}"#;

/// Opens `doc.json` holding `bytes`, switches to the text view and starts editing.
fn open_editing(cx: &mut TestAppContext, bytes: &[u8]) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let (window, view, _) = open_named(cx, "doc.json", Setup::new(bytes.to_vec()));
    press(cx, window, &view, "1");
    press(cx, window, &view, "ctrl-e");
    (window, view)
}

/// Lets the 200 ms debounce run out and the background lint land.
fn after_debounce(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<NavoWindow>) {
    cx.executor().advance_clock(Duration::from_millis(250));
    settle(cx, view);
    redraw(cx, window);
}

/// The widget's diagnostics as (byte range, severity), in text order.
fn shown(
    cx: &mut TestAppContext,
    view: &Entity<NavoWindow>,
) -> Vec<(Range<usize>, DiagnosticSeverity)> {
    let editor = editor_of(cx, view).expect("the text editor is open");
    editor.read_with(cx, |state, _| {
        state
            .diagnostics()
            .map(|set| {
                set.iter()
                    .map(|entry| (entry.range.clone(), entry.severity))
                    .collect()
            })
            .unwrap_or_default()
    })
}

fn cursor(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> usize {
    let editor = editor_of(cx, view).expect("the text editor is open");
    editor.read_with(cx, |state, _| state.cursor())
}

#[gpui_kit::test]
fn a_trailing_comma_marks_the_closing_brace_as_one_error(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, BROKEN);
    after_debounce(cx, window, &view);
    let found = shown(cx, &view);
    assert_eq!(found.len(), 1, "{found:?}");
    let (range, severity) = &found[0];
    assert_eq!(*severity, DiagnosticSeverity::Error);
    assert!(
        range.start <= 7 && 7 < range.end.max(range.start + 1),
        "{range:?}"
    );
}

#[gpui_kit::test]
fn fixing_the_text_clears_the_diagnostics_and_the_count(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, BROKEN);
    after_debounce(cx, window, &view);
    for key in ["ctrl-end", "left", "backspace"] {
        press(cx, window, &view, key);
    }
    assert_eq!(editor_text(cx, &view), r#"{"a":1}"#);
    after_debounce(cx, window, &view);
    assert!(shown(cx, &view).is_empty());
    assert_eq!(text_of(cx, window, "lint-count"), "");
}

#[gpui_kit::test]
fn f8_and_shift_f8_move_the_cursor_to_the_diagnostic_start(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, BROKEN);
    after_debounce(cx, window, &view);
    let start = shown(cx, &view)[0].0.start;
    press(cx, window, &view, "ctrl-home");
    press(cx, window, &view, "f8");
    assert_eq!(cursor(cx, &view), start);
    press(cx, window, &view, "ctrl-end");
    press(cx, window, &view, "shift-f8");
    assert_eq!(cursor(cx, &view), start);
}

#[gpui_kit::test]
fn the_header_counts_one_error(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, BROKEN);
    after_debounce(cx, window, &view);
    assert_eq!(text_of(cx, window, "lint-count"), "1 error");
}

#[gpui_kit::test]
fn a_stale_lint_never_replaces_the_result_for_newer_text(cx: &mut TestAppContext) {
    let (window, view) = open_editing(cx, br#"{"a":1}"#);
    after_debounce(cx, window, &view);
    for key in ["ctrl-end", "left", ","] {
        press_only(cx, window, key);
    }
    cx.executor().advance_clock(Duration::from_millis(150));
    for key in ["ctrl-home", "backspace", "x"] {
        press_only(cx, window, key);
    }
    after_debounce(cx, window, &view);
    let text = editor_text(cx, &view);
    assert_eq!(text, r#"x{"a":1,}"#);
    let wanted: Vec<Range<usize>> = lint(LintKind::Json, &text)
        .into_iter()
        .map(|d| d.range)
        .collect();
    let got: Vec<Range<usize>> = shown(cx, &view)
        .into_iter()
        .map(|(range, _)| range)
        .collect();
    assert!(!wanted.is_empty());
    assert_eq!(got, wanted);
}

/// A colour as the 8-bit RGBA the screen shows (the kit stores theme colours at that precision).
fn shown_rgba(color: Hsla) -> [u8; 4] {
    let rgba = Rgba::from(color);
    [rgba.r, rgba.g, rgba.b, rgba.a].map(|channel| (channel * 255.0).round() as u8)
}

#[gpui_kit::test]
fn squiggles_take_the_theme_tokens_in_both_modes(cx: &mut TestAppContext) {
    let (window, _view) = open_editing(cx, BROKEN);
    for mode in [Mode::Dark, Mode::Light] {
        cx.update(|cx| tokens::apply(mode, cx));
        redraw(cx, window);
        // The kit editor projects these four status colours into its diagnostic paint style.
        let (painted, wanted) = cx.read(|cx| {
            let status = &KitTheme::global(cx).highlight_theme.style.status;
            let colors = tokens::current(cx).colors;
            let painted = [
                status.error(cx),
                status.warning(cx),
                status.info(cx),
                status.hint(cx),
            ];
            (
                painted,
                [colors.mark, colors.warn, colors.accent, colors.muted],
            )
        });
        assert_eq!(painted.map(shown_rgba), wanted.map(shown_rgba), "{mode:?}");
    }
}
