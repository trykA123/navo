//! Tests viewer highlight selection and paged lexer mapping.

use std::sync::Arc;

use sidewise_kit::languages::lines::LineState;

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
fn rule_languages_beat_detection_in_auto_and_named() {
    check_cases([
        (
            "cfg.oil",
            &b""[..],
            LanguageChoice::Auto,
            true,
            Some("line"),
            Some("oil"),
        ),
        (
            "x.txt",
            &b""[..],
            LanguageChoice::Named("oil".into()),
            true,
            Some("line"),
            Some("oil"),
        ),
        (
            "x.a2l",
            &b""[..],
            LanguageChoice::Auto,
            true,
            Some("line"),
            Some("a2l"),
        ),
        (
            "x.txt",
            &b""[..],
            LanguageChoice::Named("a2l".into()),
            true,
            Some("line"),
            Some("a2l"),
        ),
    ]);
}

#[test]
fn the_oil_rule_lexer_colours_task_as_keyword() {
    for choice in [LanguageChoice::Auto, LanguageChoice::Named("oil".into())] {
        let highlight = choose_with(
            &Pick {
                name: "cfg.oil",
                head: b"",
                choice: &choice,
                len: None,
            },
            true,
        );
        let Highlight::Line(lexer) = highlight else {
            panic!("expected a rule lexer for {choice:?}");
        };
        let (spans, _) = lexer.lex("TASK", LineState(0));
        assert_eq!(spans, [(0..4, "keyword")], "{choice:?}");
    }
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
                len: None,
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
        len: None,
    };
    assert!(matches!(choose_with(&binary, true), Highlight::Plain));
    let source = Pick {
        name: "main.rs",
        head: b"fn\x00main",
        choice: &auto,
        len: None,
    };
    assert!(matches!(choose_with(&source, true), Highlight::Plain));
    let utf16 = Pick {
        name: "app.log",
        head: b"\xff\xfeE\x00R\x00",
        choice: &auto,
        len: None,
    };
    assert!(matches!(choose_with(&utf16, true), Highlight::Line(_)));
}

/// A UTF-32 BOM is not UTF-16, and a UTF-16 head must decode as plausible text: else Plain.
#[test]
fn only_plausible_utf16_heads_count_as_text() {
    let auto = LanguageChoice::Auto;
    let plain = |name: &str, head: &[u8]| {
        let pick = Pick {
            name,
            head,
            choice: &auto,
            len: None,
        };
        matches!(choose_with(&pick, true), Highlight::Plain)
    };
    for head in [
        &b"\xff\xfe\x00\x00T\x00\x00\x00"[..],
        b"\x00\x00\xfe\xff\x00\x00\x00T",
        b"\xff\xfeA\x00\x00\x00B\x00",
        b"\xfe\xff\x00A\x00\x00\x00B",
        b"\xff\xfe\x00\xd8A\x00",
        b"\xff\xfe\x00\xdcA\x00",
    ] {
        assert!(plain("cfg.oil", head), "{head:?}");
    }
    for head in [
        &b"\xff\xfeE\x00R\x00"[..],
        b"\xfe\xff\x00E\x00R",
        b"\xff\xfe=\xd8\x00\xdeA\x00",
        b"\xff\xfeA\x00=\xd8",
    ] {
        assert!(!plain("cfg.oil", head), "{head:?}");
    }
}

/// A trailing high surrogate is excused only when the head is a sample, shorter than the file.
#[test]
fn an_orphan_surrogate_ends_a_whole_file_but_not_a_cut_sample() {
    let auto = LanguageChoice::Auto;
    let head = b"\xff\xfeA\x00\x00\xd8";
    let plain = |len: Option<u64>| {
        let pick = Pick {
            name: "cfg.oil",
            head,
            choice: &auto,
            len,
        };
        matches!(choose_with(&pick, true), Highlight::Plain)
    };
    assert!(plain(Some(6)), "the head is the whole file");
    assert!(!plain(Some(7)), "the file goes on past the head");
    assert!(!plain(None), "an unknown length counts as a sample");
}

/// A name no lexer or rule knows takes the Auto path: binary guard first, then extension rules.
#[test]
fn an_unresolved_named_choice_falls_back_to_auto() {
    let unknown = LanguageChoice::Named("no-such-language".into());
    let binary = Pick {
        name: "main.rs",
        head: b"fn\x00main",
        choice: &unknown,
        len: None,
    };
    assert!(matches!(choose_with(&binary, true), Highlight::Plain));
    check_cases([
        (
            "cfg.oil",
            &b""[..],
            unknown.clone(),
            true,
            Some("line"),
            Some("oil"),
        ),
        (
            "main.rs",
            &b""[..],
            unknown,
            true,
            Some("rust"),
            Some("log"),
        ),
    ]);
}

#[test]
fn two_picks_of_one_rule_language_share_one_compiled_lexer() {
    let pick = Pick {
        name: "a.oil",
        head: b"",
        choice: &LanguageChoice::Auto,
        len: None,
    };
    let (Highlight::Line(first), Highlight::Line(second)) =
        (choose_with(&pick, true), choose_with(&pick, true))
    else {
        panic!("oil is a rule language");
    };
    assert!(
        Arc::ptr_eq(&first, &second),
        "a second pick must reuse the first lexer"
    );
}
