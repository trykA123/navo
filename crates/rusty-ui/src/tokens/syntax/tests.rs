use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::component::theme::Theme as KitTheme;
use gpui_kit::{FontStyle, FontWeight, TestAppContext};
use rusty_syntax::{LineState, TokenKind, builtin, lex_line};

use super::{FALLBACKS, SyntaxTheme, highlight_theme, roles};
use crate::languages::HIGHLIGHT_NAMES;
use crate::tokens::{Density, Mode, Theme, apply, apply_look, contrast_ratio, current};

const MODES: [Mode; 2] = [Mode::Light, Mode::Dark];
const MINIMUM_CONTRAST: f32 = 4.5;
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

fn built(syntax: SyntaxTheme, mode: Mode) -> (HighlightTheme, Theme) {
    let theme = Theme::new(mode);
    let base = HighlightTheme::default_light();
    (highlight_theme(syntax, mode, &theme.colors, &base), theme)
}

fn every_name() -> Vec<&'static str> {
    HIGHLIGHT_NAMES
        .iter()
        .copied()
        .chain(KINDS.map(TokenKind::highlight_name))
        .collect()
}

#[test]
fn each_theme_colours_every_highlight_name_in_both_modes() {
    for syntax in SyntaxTheme::ALL {
        for mode in MODES {
            let (theme, _) = built(syntax, mode);
            for name in every_name() {
                let colour = theme.style(name).and_then(|style| style.color);
                assert!(
                    colour.is_some(),
                    "{syntax:?} {mode:?}: {name} has no colour"
                );
            }
        }
    }
}

#[test]
fn each_code_token_meets_aa_contrast_on_the_viewer_background() {
    for syntax in SyntaxTheme::ALL {
        for mode in MODES {
            let (theme, ours) = built(syntax, mode);
            for name in every_name() {
                let colour = theme.style(name).and_then(|style| style.color);
                let ratio =
                    colour.map_or(0., |colour| contrast_ratio(colour, ours.colors.background));
                assert!(
                    ratio >= MINIMUM_CONTRAST,
                    "{syntax:?} {mode:?}: {name} is {ratio:.2}:1"
                );
            }
        }
    }
}

#[test]
fn every_fallback_points_at_a_role_the_theme_defines() {
    for syntax in SyntaxTheme::ALL {
        for mode in MODES {
            let all = roles(syntax, mode, &Theme::new(mode).colors);
            for fallback in &FALLBACKS {
                assert!(
                    all.iter().any(|role| role.name == fallback.from),
                    "{}: {} falls back to {}",
                    syntax.toml(),
                    fallback.name,
                    fallback.from
                );
            }
        }
    }
}

#[test]
fn styles_follow_the_role_table() {
    let (theme, _) = built(SyntaxTheme::OxidisedIron, Mode::Light);
    let style = |name: &str| theme.style(name).expect("a style");
    assert_eq!(style("comment").font_style, Some(FontStyle::Italic));
    assert_eq!(
        style("variable.special").font_style,
        Some(FontStyle::Italic)
    );
    assert_eq!(style("string.escape").font_weight, Some(FontWeight::BOLD));
    assert_eq!(style("title").font_weight, Some(FontWeight::BOLD));
    assert_eq!(style("emphasis").font_style, Some(FontStyle::Italic));
    assert_eq!(style("emphasis.strong").font_weight, Some(FontWeight::BOLD));
    assert_eq!(style("keyword").font_style, None);
    assert_eq!(style("keyword").font_weight, None);
}

#[test]
fn the_two_themes_colour_keywords_differently() {
    for mode in MODES {
        let colour = |syntax| built(syntax, mode).0.style("keyword").and_then(|s| s.color);
        assert_ne!(
            colour(SyntaxTheme::OxidisedIron),
            colour(SyntaxTheme::Mocha)
        );
    }
}

#[test]
fn the_editor_surface_uses_the_ferrite_tokens() {
    let (theme, ours) = built(SyntaxTheme::OxidisedIron, Mode::Dark);
    assert_eq!(theme.style.editor_background, Some(ours.colors.background));
    assert_eq!(theme.style.editor_foreground, Some(ours.colors.text));
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

#[gpui_kit::test]
fn applying_a_syntax_theme_installs_it_as_the_kit_highlight_theme(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        for syntax in SyntaxTheme::ALL {
            for mode in MODES {
                apply_look(mode, Density::Normal, syntax, cx);
                let (expected, _) = built(syntax, mode);
                let installed = &KitTheme::global(cx).highlight_theme;
                for name in every_name() {
                    assert_eq!(installed.style(name), expected.style(name), "{name}");
                }
                assert_eq!(current(cx).syntax, syntax);
            }
        }
    });
}

#[gpui_kit::test]
fn changing_the_mode_keeps_the_syntax_theme(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        apply_look(Mode::Light, Density::Normal, SyntaxTheme::Mocha, cx);
        apply(Mode::Dark, cx);
        assert_eq!(current(cx).syntax, SyntaxTheme::Mocha);
        let (expected, _) = built(SyntaxTheme::Mocha, Mode::Dark);
        let keyword = KitTheme::global(cx).highlight_theme.style("keyword");
        assert_eq!(keyword, expected.style("keyword"));
    });
}

#[test]
fn link_uri_is_not_underlined_until_the_kit_supports_it() {
    for syntax in SyntaxTheme::ALL {
        for mode in MODES {
            let (theme, _) = built(syntax, mode);
            let style = theme.style("link_uri").expect("a style");
            assert_eq!(style.font_style, None, "{syntax:?} {mode:?}");
        }
    }
}

#[test]
fn each_code_token_meets_aa_contrast_on_the_active_line() {
    for syntax in SyntaxTheme::ALL {
        for mode in MODES {
            let (theme, _) = built(syntax, mode);
            let line = theme.style.editor_active_line.expect("an active line");
            for name in every_name() {
                let colour = theme.style(name).and_then(|style| style.color);
                let ratio = colour.map_or(0., |colour| contrast_ratio(colour, line));
                assert!(
                    ratio >= MINIMUM_CONTRAST,
                    "{syntax:?} {mode:?}: {name} is {ratio:.2}:1 on the active line"
                );
            }
        }
    }
}
