use std::sync::Arc;

use gpui_kit::component::highlighter::LanguageRegistry;
use gpui_kit::component::theme::Theme as KitTheme;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use navo_fs::FakeFs;

use super::{Setup, editor_of, redraw, window_on};
use crate::window_view::NavoWindow;
use crate::window_view::tests::{press, type_in};
use crate::window_view::viewer::editor::run_probe;

/// Opens `name` (holding `text`) with F3 and returns the window and view, with no grammar check.
fn open_with_f3(
    cx: &mut TestAppContext,
    name: &str,
    text: &str,
) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file(&format!("home/{name}"), 3);
    fs.add_folder("dest");
    let setup = Setup::new(text.as_bytes().to_vec());
    let (window, view, _) = window_on(cx, Arc::new(fs), setup);
    press(cx, window, &view, "down");
    press(cx, window, &view, "f3");
    redraw(cx, window);
    (window, view)
}

/// The editor's language name in the open viewer.
fn language_name(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> String {
    let editor = editor_of(cx, view).expect("the text editor is open");
    editor.read_with(cx, |state, _| state.language_name().to_string())
}

/// Whether the editor was asked to draw `word` in the theme's style for `role`.
fn drew_role(cx: &mut TestAppContext, window: AnyWindowHandle, word: &str, role: &str) -> bool {
    redraw(cx, window);
    let wanted = cx.read(|cx| KitTheme::global(cx).highlight_theme.style(role));
    let wanted = wanted.expect("the theme styles the role");
    run_probe::take()
        .iter()
        .any(|(shown, style)| shown.contains(word) && *style == wanted)
}

/// Whether the editor was asked to draw `word` in the theme's `title` style.
fn drew_title(cx: &mut TestAppContext, window: AnyWindowHandle, word: &str) -> bool {
    drew_role(cx, window, word, "title")
}

/// Opens `name` with F3, switching to Text if a Rich view opened, and checks it has a grammar.
fn language_after_f3(cx: &mut TestAppContext, name: &str, text: &str) -> String {
    let (window, view) = open_with_f3(cx, name, text);
    if editor_of(cx, &view).is_none() {
        press(cx, window, &view, "1");
    }
    let language = language_name(cx, &view);
    let config = LanguageRegistry::singleton().language(&language);
    assert!(
        config.is_some_and(|config| config.has_grammar()),
        "{language}"
    );
    language
}

#[gpui_kit::test]
fn f3_on_a_powershell_script_uses_the_powershell_grammar(cx: &mut TestAppContext) {
    let language = language_after_f3(cx, "build.ps1", "Write-Host \"hi\"\n");
    assert_eq!(language, "powershell");
}

#[gpui_kit::test]
fn f3_on_an_arxml_file_uses_the_xml_grammar(cx: &mut TestAppContext) {
    let language = language_after_f3(cx, "ecu.arxml", "<AUTOSAR></AUTOSAR>\n");
    assert_eq!(language, "xml");
}

const LOG: &str = "ERROR disk full\nstarted\n";

#[gpui_kit::test]
fn f3_on_a_log_file_colours_the_level_word(cx: &mut TestAppContext) {
    let (window, view) = open_with_f3(cx, "app.log", LOG);
    assert_eq!(language_name(cx, &view), "log");
    assert!(drew_title(cx, window, "ERROR"));
}

#[gpui_kit::test]
fn ctrl_e_on_a_log_file_keeps_the_language_and_colour(cx: &mut TestAppContext) {
    let (window, view) = open_with_f3(cx, "app.log", LOG);
    redraw(cx, window);
    run_probe::take();
    press(cx, window, &view, "ctrl-e");
    press(cx, window, &view, "ctrl-home");
    type_in(cx, window, &view, "WARN: y\n");
    assert_eq!(language_name(cx, &view), "log");
    assert!(drew_role(cx, window, "WARN", "keyword"));
}

#[gpui_kit::test]
fn a_12_kib_log_line_opens_without_colour_runs_for_that_line(cx: &mut TestAppContext) {
    let text = format!("ERROR {}\nWARN: y\n", "x".repeat(12 * 1024));
    let (window, view) = open_with_f3(cx, "big.log", &text);
    assert_eq!(language_name(cx, &view), "log");
    redraw(cx, window);
    let runs = run_probe::take();
    assert!(
        runs.iter().all(|(shown, _)| !shown.contains("ERROR")),
        "long line stays plain"
    );
    assert!(
        runs.iter().any(|(shown, _)| shown.contains("WARN")),
        "the short line after it is still coloured: {runs:?}"
    );
    assert!(
        runs.iter().all(|(shown, _)| !shown.contains("xxxxxxxx")),
        "no runs cover the long line"
    );
}

#[gpui_kit::test]
fn f3_on_a_txt_file_uses_the_log_lexer(cx: &mut TestAppContext) {
    let (_, view) = open_with_f3(cx, "notes.txt", LOG);
    assert_eq!(language_name(cx, &view), "log");
}

#[gpui_kit::test]
fn f3_on_a_small_arxml_opens_the_tree_and_alt_1_shows_xml(cx: &mut TestAppContext) {
    let (window, view) = open_with_f3(cx, "ecu.arxml", "<AUTOSAR></AUTOSAR>\n");
    assert!(editor_of(cx, &view).is_none(), "F3 keeps the Tree (D18)");
    press(cx, window, &view, "alt-1");
    redraw(cx, window);
    assert_eq!(language_name(cx, &view), "xml");
}
