use crate::l10n::scan::live_tokens;
use crate::l10n::tokens::{Spanned, Token, tokenize};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Found {
    pub line: usize,
    pub what: String,
}

const CONSTRUCTORS: [&str; 6] = ["rgb", "rgba", "hsla", "hsl", "oklch", "opaque_grey"];
const NAMED: [&str; 7] = [
    "black",
    "white",
    "red",
    "green",
    "blue",
    "yellow",
    "opaque_grey",
];
const LITERAL_TYPES: [&str; 2] = ["Hsla", "Rgba"];

pub(super) fn scan_rust(source: &str) -> Vec<Found> {
    let tokens = live_tokens(tokenize(source));
    let mut found = Vec::new();
    for (index, spanned) in tokens.iter().enumerate() {
        match &spanned.token {
            Token::Ident(name) => {
                if let Some(what) = ident_hit(&tokens, index, name) {
                    found.push(Found {
                        line: spanned.line,
                        what,
                    });
                }
            }
            Token::Str(text) => {
                if let Some(what) = svg_hit(text) {
                    found.push(Found {
                        line: spanned.line,
                        what,
                    });
                }
            }
            Token::Punct(_) => {}
        }
    }
    found
}

pub(super) fn scan_svg(text: &str) -> Vec<Found> {
    text.lines()
        .enumerate()
        .filter_map(|(at, line)| {
            Some(Found {
                line: at + 1,
                what: svg_hit(line)?,
            })
        })
        .collect()
}

fn svg_hit(text: &str) -> Option<String> {
    ["fill=\"#", "stroke=\"#"]
        .into_iter()
        .find(|needle| text.contains(needle))
        .map(|needle| format!("{needle}…"))
}

fn punct(tokens: &[Spanned], index: usize, wanted: char) -> bool {
    matches!(tokens.get(index), Some(s) if s.token == Token::Punct(wanted))
}

fn is_call(tokens: &[Spanned], index: usize) -> bool {
    let before = index.checked_sub(1).and_then(|at| tokens.get(at));
    let is_method_or_decl = match before.map(|s| &s.token) {
        Some(Token::Punct('.')) => true,
        Some(Token::Ident(word)) => word == "fn",
        _ => false,
    };
    punct(tokens, index + 1, '(') && !is_method_or_decl
}

fn ident_hit(tokens: &[Spanned], index: usize, name: &str) -> Option<String> {
    if CONSTRUCTORS.contains(&name) && is_call(tokens, index) {
        return Some(format!("{name}("));
    }
    if NAMED.contains(&name) && is_call(tokens, index) && punct(tokens, index + 2, ')') {
        return Some(format!("{name}()"));
    }
    let is_literal = LITERAL_TYPES.contains(&name)
        && punct(tokens, index + 1, '{')
        && matches!(
            tokens.get(index + 2).map(|s| &s.token),
            Some(Token::Ident(_))
        )
        && punct(tokens, index + 3, ':')
        && !punct(tokens, index + 4, ':');
    is_literal.then(|| format!("{name} {{"))
}
