//! Tests viewer highlight selection and paged lexer mapping.

use super::{Highlight, LanguageChoice, Pick, choose_with};
use crate::window_view::tests::press;
use crate::window_view::viewer::EDIT_LIMIT;
use crate::window_view::viewer::Kinds;
use crate::window_view::viewer::tests::structured::fake_with;
use crate::window_view::viewer::tests::{
    Log, Setup, hooks, paged, press_only, redraw, window_with,
};

#[test]
fn detected_languages_select_highlights() {
    check_cases([
        (
            "ecu.arxml",
            &b""[..],
            LanguageChoice::Auto,
            true,
            Some("xml"),
            Some("xml"),
        ),
        (
            "big.json",
            &b""[..],
            LanguageChoice::Auto,
            true,
            Some("json"),
            Some("json"),
        ),
        (
            "main.rs",
            &b""[..],
            LanguageChoice::Auto,
            true,
            Some("rust"),
            Some("log"),
        ),
        (
            "x.dat",
            &b"<?xml"[..],
            LanguageChoice::Auto,
            true,
            Some("xml"),
            Some("xml"),
        ),
    ]);
}

#[test]
fn choices_and_switch_override_detection() {
    check_cases([
        (
            "ecu.arxml",
            &b""[..],
            LanguageChoice::Plain,
            true,
            None,
            None,
        ),
        (
            "x.txt",
            &b""[..],
            LanguageChoice::Named("JSON".into()),
            true,
            Some("json"),
            Some("json"),
        ),
        (
            "x.txt",
            &b""[..],
            LanguageChoice::Named("log".into()),
            true,
            Some("line"),
            Some("log"),
        ),
        (
            "ecu.arxml",
            &b""[..],
            LanguageChoice::Auto,
            false,
            None,
            None,
        ),
    ]);
}

type Case<'a> = (
    &'a str,
    &'a [u8],
    LanguageChoice,
    bool,
    Option<&'a str>,
    Option<&'a str>,
);

/// Checks selection and paged lexer names for each case.
fn check_cases<const N: usize>(cases: [Case<'_>; N]) {
    for (name, head, choice, enabled, expected, paged) in cases {
        let highlight = choose_with(
            &Pick {
                name,
                head,
                choice: &choice,
            },
            enabled,
        );
        let selected = match &highlight {
            Highlight::Grammar(name) => Some(*name),
            Highlight::Line(_) => Some("line"),
            Highlight::Plain => None,
        };
        assert_eq!(selected, expected, "selection for {name} {choice:?}");
        assert_eq!(
            highlight.paged_lexer().as_deref().map(|lexer| lexer.name()),
            paged,
            "paged lexer for {name} {choice:?}"
        );
    }
}

/// Asserts that the paged view's first row has a `tag` span.
fn assert_tag(
    cx: &mut gpui_kit::TestAppContext,
    view: &gpui_kit::Entity<crate::window_view::NavoWindow>,
) {
    let view = paged(cx, view);
    let spans = view.read_with(cx, |view, _| view.row_spans(0));
    assert!(spans.iter().any(|(_, role)| *role == "tag"), "{spans:?}");
}

#[gpui_kit::test]
fn a_hex_born_paged_view_reused_as_text_is_coloured(cx: &mut gpui_kit::TestAppContext) {
    let bytes = b"<A b=\"1\"><!-- c --></A>\n".to_vec();
    let files = hooks(Setup::huge(bytes), &Log::default());
    let fs = fake_with("big.arxml", EDIT_LIMIT + 1);
    let (window, view) = window_with(cx, fs, files, Kinds::builtin());
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    redraw(cx, window);
    press_only(cx, window, "alt-3");
    redraw(cx, window);
    press_only(cx, window, "alt-1");
    redraw(cx, window);
    assert_tag(cx, &view);
}

#[gpui_kit::test]
fn the_editor_followed_as_paged_text_is_coloured(cx: &mut gpui_kit::TestAppContext) {
    let bytes = b"<A b=\"1\"><!-- c --></A>\n".to_vec();
    let files = hooks(Setup::new(bytes), &Log::default());
    let (window, view) = window_with(cx, fake_with("app.xml", 24), files, Kinds::builtin());
    press(cx, window, &view, "down");
    press_only(cx, window, "f3");
    redraw(cx, window);
    press_only(cx, window, "alt-1");
    redraw(cx, window);
    press_only(cx, window, "alt-f");
    redraw(cx, window);
    assert_tag(cx, &view);
}

/// A head with a NUL byte and no UTF-16 BOM is binary: Auto gives Plain, not the Log lexer.
#[test]
fn a_binary_head_gets_no_line_lexer() {
    let auto = LanguageChoice::Auto;
    let binary = Pick {
        name: "big.bin",
        head: b"\x00\x07\x0e text",
        choice: &auto,
    };
    assert!(matches!(choose_with(&binary, true), Highlight::Plain));
    let source = Pick {
        name: "main.rs",
        head: b"fn\x00main",
        choice: &auto,
    };
    assert!(matches!(choose_with(&source, true), Highlight::Plain));
    let utf16 = Pick {
        name: "app.log",
        head: b"\xff\xfeE\x00R\x00",
        choice: &auto,
    };
    assert!(matches!(choose_with(&utf16, true), Highlight::Line(_)));
}
