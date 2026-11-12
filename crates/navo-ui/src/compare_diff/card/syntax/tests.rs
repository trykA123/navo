//! Tests for the Stream card's syntax colours (D38.3): a parsed line takes its colour runs from
//! its own hunk snippet, a card with no parse or no grammar keeps drawing plain text, and a line
//! whose text column is tinted takes the on-tint variant of every colour (D38.4).

use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::{HighlightStyle, Hsla};

use super::{CardKey, CardLook, CardSyntax, TextRun};
use crate::compare_diff::{Card, CardRow, DiffRow, HunkView, PairKind, ViewedKey};
use crate::tokens::{Mode, Theme, TintLook, highlight_theme, on_tint};
use crate::window_view::EDIT_LIMIT;

/// The left lines of the card's one hunk: an equal comment, a changed keyword line and a deleted
/// comment.
const LEFT: [&str; 3] = ["// note", "fn main() {", "// note"];

/// The right lines of the same hunk; the deleted left line has no right text.
const RIGHT: [&str; 3] = ["// note", "fn main() {}", "let x = 1;"];

/// One card line with the same text on both sides.
fn line(row: DiffRow, text: &str, right: Option<&str>) -> CardRow {
    CardRow::Line {
        row,
        hunk: 0,
        left: Some(text.to_owned()),
        right: right.map(str::to_owned),
    }
}

/// The card's rows: one hunk, four lines, the last two one-sided.
fn card() -> Card {
    let rows = vec![
        line(DiffRow::Equal(0, 0), LEFT[0], Some(RIGHT[0])),
        line(DiffRow::Changed(1, 1), LEFT[1], Some(RIGHT[1])),
        line(DiffRow::Deleted(2), LEFT[2], None),
        CardRow::Line {
            row: DiffRow::Inserted(2),
            hunk: 0,
            left: None,
            right: Some(RIGHT[2].to_owned()),
        },
        CardRow::Gap { lines: 4 },
    ];
    Card {
        kind: PairKind::Text,
        rows,
        hunks: vec![HunkView {
            rows: 0..4,
            left_bytes: 0..0,
            right_bytes: 0..0,
            digest: None,
            added: 1,
            removed: 1,
        }],
        added: 2,
        removed: 2,
        viewed_key: ViewedKey([7; 64]),
        viewed: false,
        viewable: true,
    }
}

/// The card parsed through the named language: the hunk's two sides as documents of their own.
fn parsed(name: &str) -> CardSyntax {
    CardSyntax::parse(name, &card(), EDIT_LIMIT)
}

/// The style of the run that holds byte `at` of the line; the default when nothing colours it.
fn style_at(runs: &[TextRun], at: usize) -> HighlightStyle {
    runs.iter()
        .find(|(run, _)| run.contains(&at))
        .map_or_else(HighlightStyle::default, |(_, style)| *style)
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

/// The look of the card `syntax` with the theme's colours.
fn look<'a>(syntax: &'a CardSyntax, colours: &'a HighlightTheme) -> CardLook<'a> {
    CardLook::new(Some(syntax), Some(colours)).expect("a card look")
}

#[test]
fn a_parsed_card_line_takes_its_own_hunk_snippet_colours() {
    let t = Theme::new(Mode::Light);
    let colours = highlight_theme(
        t.syntax,
        t.mode,
        &t.colors,
        &HighlightTheme::default_light(),
    );
    let card = card();
    let syntax = parsed("main.rs");
    let look = look(&syntax, &colours);
    let (left, right) = look.line_runs(&t, &card.rows, 1);
    assert!(
        !left.is_empty() && style_at(&left, 0).color.is_some(),
        "the `fn` line of the left snippet has a colour run"
    );
    assert_eq!(
        style_at(&left, 0).color,
        expected(&colours, &t, ("keyword", true)),
        "a changed line takes the on-tint keyword colour"
    );
    assert!(
        !right.is_empty(),
        "the same row's right line colours from its own snippet"
    );
}

#[test]
fn a_card_with_no_parse_or_no_grammar_draws_plain_text() {
    let t = Theme::new(Mode::Light);
    let colours = HighlightTheme::default_light();
    let card = card();
    assert!(
        CardLook::new(None, Some(&colours)).is_none(),
        "a frame without a parse asks for no colours"
    );
    let plain = parsed("notes.txt");
    let look = look(&plain, &colours);
    let (left, right) = look.line_runs(&t, &card.rows, 1);
    assert!(
        left.is_empty() && right.is_empty(),
        "a text file has no grammar, so its lines draw as before"
    );
    let gap = look.line_runs(&t, &card.rows, 4);
    assert!(
        gap.0.is_empty() && gap.1.is_empty(),
        "a gap row shows no code and colours nothing"
    );
    // The very same row of the same card does colour once its snippet is parsed: the empty
    // answer above is the missing grammar, not a lookup that never colours.
    let rust = parsed("main.rs");
    let coloured = CardLook::new(Some(&rust), Some(&colours)).expect("a card look");
    assert!(
        !coloured.line_runs(&t, &card.rows, 1).0.is_empty(),
        "the Rust parse of the same card colours row 1"
    );
}

#[test]
fn a_deleted_card_line_takes_the_on_tint_comment_colour() {
    let t = Theme::new(Mode::Dark);
    let colours = highlight_theme(t.syntax, t.mode, &t.colors, &HighlightTheme::default_dark());
    let card = card();
    let syntax = parsed("main.rs");
    let look = look(&syntax, &colours);
    let base = expected(&colours, &t, ("comment", false));
    let on_tint = expected(&colours, &t, ("comment", true));
    assert!(
        base.is_some() && base != on_tint,
        "the theme tints comments"
    );
    let (kept, _) = look.line_runs(&t, &card.rows, 0);
    assert_eq!(
        style_at(&kept, 2).color,
        base,
        "an equal line keeps the plain comment colour"
    );
    let (deleted, right) = look.line_runs(&t, &card.rows, 2);
    assert_eq!(
        style_at(&deleted, 2).color,
        on_tint,
        "the deleted line's comment takes the on-tint colour"
    );
    assert!(
        right.is_empty(),
        "a deleted line has no right text to colour"
    );
}

#[test]
fn a_card_key_is_the_held_card_and_generation_only() {
    let held = std::sync::Arc::new(card());
    let key = CardKey::new(1, 7, &held);
    assert!(key.same(&key), "a key names its own card");
    let other = std::sync::Arc::new(card());
    assert!(
        !key.same(&CardKey::new(1, 7, &other)),
        "a rebuilt card is another key"
    );
    assert!(
        !CardKey::new(1, 8, &held).same(&key),
        "another generation is another key"
    );
    assert!(
        !CardKey::new(2, 7, &held).same(&key),
        "another item is another key"
    );
}
