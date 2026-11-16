//! Tests that tie the NV lexer's token kinds to the kit's syntax themes.
//! They live outside `tokens` so the kit never needs `navo_syntax`.

use gpui_kit::component::highlighter::HighlightTheme;
use navo_syntax::{LineState, TokenKind, builtin, lex_line};

use crate::tokens::{ColorTheme, Mode, SyntaxTheme, Theme, ThemeLook, highlight_theme};

const KINDS: [TokenKind; 13] = [
    TokenKind::Keyword,
    TokenKind::Type,
    TokenKind::Function,
    TokenKind::Constant,
    TokenKind::Attribute,
    TokenKind::Tag,
    TokenKind::Property,
    TokenKind::Variable,
    TokenKind::Comment,
    TokenKind::String,
    TokenKind::Number,
    TokenKind::Operator,
    TokenKind::Delimiter,
];

/// Builds the highlight theme and Navo theme for one syntax theme and mode.
fn built(syntax: SyntaxTheme, mode: Mode) -> (HighlightTheme, Theme) {
    let look = ThemeLook {
        color_theme: ColorTheme::default(),
        ..ThemeLook::default()
    };
    let theme = Theme::with_look(mode, look);
    let base = HighlightTheme::default_light();
    (highlight_theme(syntax, mode, &theme.colors, &base), theme)
}

#[test]
fn lexer_kinds_keep_the_names_the_kit_colours() {
    assert_eq!(
        KINDS.map(TokenKind::highlight_name),
        [
            "keyword",
            "type",
            "function",
            "constant",
            "attribute",
            "tag",
            "property",
            "variable.special",
            "comment",
            "string",
            "number",
            "operator",
            "punctuation.delimiter",
        ]
    );
}

#[test]
fn lexer_tokens_of_the_built_in_languages_reach_the_theme() {
    let languages = builtin();
    assert!(!languages.is_empty());
    for syntax in SyntaxTheme::ALL {
        let (theme, _) = built(syntax, Mode::Light);
        for language in &languages {
            let words: Vec<&str> = language
                .keywords
                .iter()
                .flat_map(|group| group.words.iter().map(String::as_str))
                .collect();
            let (tokens, _) = lex_line(language, &words.join(" "), LineState::Normal);
            assert!(!tokens.is_empty(), "{} lexes to nothing", language.name);
            for token in tokens {
                let colour = theme
                    .style(token.kind.highlight_name())
                    .and_then(|style| style.color);
                assert!(colour.is_some(), "{}: {:?}", language.name, token.kind);
            }
        }
    }
}
