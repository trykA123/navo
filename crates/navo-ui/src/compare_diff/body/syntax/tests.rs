//! Tests for syntax colours in the file diff: highlighter runs under the word fill, comments
//! on a tinted line, and the size cap.

use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::{HighlightStyle, Hsla};

use super::{PairSyntax, SideSyntax, SyntaxLook};
use crate::compare_diff::body::cut::{LineAt, shown_line};
use crate::compare_diff::body::{BodyLook, RowCut, Side};
use crate::compare_diff::{
    DiffRow, Layout, PairFacts, PairText, RowSpans, ShownKind, SpanCache, ViewedKey,
};
use crate::languages::register_languages;
use crate::tokens::{Mode, Theme, TintLook, highlight_theme, on_tint};
use crate::window_view::EDIT_LIMIT;

const RUST: [&str; 4] = ["fn main() {", "    let s = \"hi\";", "    // note", "}"];

/// The Rust lines above as both sides of a pair whose row 1 changed.
fn pair() -> PairText {
    let lines: Vec<String> = RUST.iter().map(|line| (*line).to_owned()).collect();
    PairText {
        left_starts: Vec::new(),
        right_starts: Vec::new(),
        left: lines.clone(),
        right: lines,
        rows: vec![
            DiffRow::Equal(0, 0),
            DiffRow::Changed(1, 1),
            DiffRow::Equal(2, 2),
        ],
        hunks: vec![],
        facts: PairFacts {
            left_encoding: "",
            right_encoding: "",
            left_endings: "",
            right_endings: "",
            raw_equal: false,
            display_equal: false,
            fallbacks: vec![],
        },
        viewed: ViewedKey([0; 64]),
    }
}

/// Both sides parsed as Rust.
fn parsed(text: &PairText) -> PairSyntax {
    register_languages();
    PairSyntax {
        left: SideSyntax::parse("rust", &text.left, EDIT_LIMIT),
        right: SideSyntax::parse("rust", &text.right, EDIT_LIMIT),
    }
}

/// The style of the run that holds byte `at` of `line`'s shown text.
fn style_at(look: &BodyLook, line: &LineAt, at: usize) -> HighlightStyle {
    let shown = shown_line(look, line, &RowCut::default()).expect("a line");
    shown
        .runs
        .iter()
        .find(|(run, _)| run.contains(&at))
        .map(|(_, style)| *style)
        .unwrap_or_default()
}

/// The colour F3 gives `name`, moved for a tinted line as `on_tint` says.
fn expected(colours: &HighlightTheme, t: &Theme, (name, tinted): (&str, bool)) -> Option<Hsla> {
    let base = colours.style(name)?.color?;
    let look = TintLook {
        mode: t.mode,
        syntax: t.syntax,
    };
    Some(on_tint(name, base, tinted, look))
}

#[test]
fn a_rust_line_gets_keyword_and_string_colours_under_the_word_fill() {
    let t = Theme::new(Mode::Light);
    let colours = highlight_theme(
        t.syntax,
        t.mode,
        &t.colors,
        &HighlightTheme::default_light(),
    );
    let text = pair();
    let syntax = parsed(&text);
    let mut spans = SpanCache::default();
    spans.request(0..2);
    let word = 13..15;
    let found = RowSpans {
        row: 1,
        left: vec![word],
        right: vec![],
    };
    spans.insert(&text, vec![found]);
    let look = BodyLook {
        theme: &t,
        layout: Layout::SideBySide,
        digits: 1,
        current: None,
        spans: Some(&spans),
        ticks: None,
        bytes_here: None,
        syntax: SyntaxLook::new(Some(&syntax), Some(&colours)),
    };
    let line = LineAt {
        kind: ShownKind::Changed,
        side: Side::Left,
        line: Some((1, RUST[1])),
        slot: Side::Left,
    };
    let keyword = style_at(&look, &line, 5);
    assert_eq!(keyword.color, expected(&colours, &t, ("keyword", true)));
    assert!(keyword.color.is_some(), "`let` is coloured");
    let word = style_at(&look, &line, 13);
    assert_eq!(word.color, expected(&colours, &t, ("string", true)));
    assert!(word.color.is_some(), "the string is coloured");
    assert_eq!(word.background_color, Some(t.diff.change.fill));
    assert_eq!(style_at(&look, &line, 12).background_color, None);
}

#[test]
fn a_comment_on_a_tinted_line_takes_comment_on_tint() {
    let t = Theme::new(Mode::Dark);
    let colours = highlight_theme(t.syntax, t.mode, &t.colors, &HighlightTheme::default_dark());
    let text = pair();
    let syntax = parsed(&text);
    let look = BodyLook {
        theme: &t,
        layout: Layout::Unified,
        digits: 1,
        current: None,
        spans: None,
        ticks: None,
        bytes_here: None,
        syntax: SyntaxLook::new(Some(&syntax), Some(&colours)),
    };
    let at = |kind| LineAt {
        kind,
        side: Side::Right,
        line: Some((2, RUST[2])),
        slot: Side::Left,
    };
    let base = expected(&colours, &t, ("comment", false));
    let on_tint = expected(&colours, &t, ("comment", true));
    assert!(base.is_some() && base != on_tint);
    assert_eq!(style_at(&look, &at(ShownKind::Inserted), 6).color, on_tint);
    assert_eq!(style_at(&look, &at(ShownKind::Equal), 6).color, base);
}

#[test]
fn a_side_over_the_edit_limit_stays_plain() {
    register_languages();
    let small = vec!["fn main() {}".to_owned()];
    assert!(SideSyntax::parse("rust", &small, EDIT_LIMIT).is_some());
    assert!(
        SideSyntax::parse("text", &small, EDIT_LIMIT).is_none(),
        "plain text"
    );
    let mebibyte = "a".repeat(1 << 20);
    let over = vec![mebibyte; 16];
    assert!(
        SideSyntax::parse("rust", &over, EDIT_LIMIT).is_none(),
        "16 MiB plus 16 line ends is over the cap"
    );
}

#[test]
fn a_line_that_is_not_the_parsed_line_stays_plain() {
    let text = pair();
    let syntax = parsed(&text);
    let colours = HighlightTheme::default_light();
    let side = syntax.left.as_ref().expect("a parsed side");
    assert!(!side.line_runs((1, RUST[1]), &*colours).is_empty());
    assert!(side.line_runs((1, "let x"), &*colours).is_empty());
    assert!(side.line_runs((9, RUST[1]), &*colours).is_empty());
}
