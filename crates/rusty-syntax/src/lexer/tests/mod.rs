use super::*;
use crate::{Delimiter, KeywordGroup, TokenKind as K};

mod props;
mod rules;

fn owned(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

fn delimiter(open: &str, close: &str, escape: Option<char>, multiline: bool) -> Delimiter {
    Delimiter {
        open: open.to_owned(),
        close: close.to_owned(),
        escape,
        multiline,
        kind: K::String,
    }
}

fn sample() -> Language {
    Language {
        name: "sample".to_owned(),
        extensions: owned(&["smp"]),
        case_ignored: false,
        keywords: vec![
            KeywordGroup {
                kind: K::Keyword,
                words: owned(&["if", "else", "while"]),
                prefix: false,
            },
            KeywordGroup {
                kind: K::Constant,
                words: owned(&["TRUE"]),
                prefix: false,
            },
            KeywordGroup {
                kind: K::Function,
                words: owned(&["dsGet"]),
                prefix: true,
            },
        ],
        line_comments: owned(&["//", "#"]),
        block_comments: vec![
            ("/*".to_owned(), "*/".to_owned()),
            ("(*".to_owned(), "*)".to_owned()),
        ],
        delimiters: vec![
            delimiter("\"", "\"", Some('\\'), false),
            delimiter("'''", "'''", None, true),
        ],
        operators: owned(&["==", "=", "+", "->", "/"]),
        numbers: true,
    }
}

fn spans<'a>(lang: &Language, line: &'a str, state: LineState) -> (Vec<(&'a str, K)>, LineState) {
    let (tokens, next) = lex_line(lang, line, state);
    let spans = tokens
        .iter()
        .map(|t| {
            (
                line.get(t.range.clone())
                    .unwrap_or("<not on char boundaries>"),
                t.kind,
            )
        })
        .collect();
    (spans, next)
}

fn tokens<'a>(lang: &Language, line: &'a str) -> Vec<(&'a str, K)> {
    spans(lang, line, LineState::Normal).0
}
