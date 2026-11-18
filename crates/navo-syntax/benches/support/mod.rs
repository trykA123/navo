//! Deterministic fixtures for the rule lexer benches, also built by tests/bench_fixtures.rs.

#![allow(
    clippy::unwrap_used,
    reason = "a broken fixture must stop the run, not measure an error path"
)]

use navo_syntax::{Language, Lexer, LineState, from_udl};

/// The line-token cap, mirrored from `lexer/spans.rs` because that module is private.
pub const MAX_TOKENS: usize = 4096;

/// One prepared page: the rule lexer and its rows.
pub struct Page {
    pub lexer: Lexer,
    pub rows: Vec<String>,
}

/// Lexes every row with carried state and returns the byte and token counts.
pub fn lex_page(page: &Page) -> (usize, usize) {
    let mut state = LineState::Normal;
    let mut bytes = 0;
    let mut tokens = 0;
    for row in &page.rows {
        let (found, next) = page.lexer.line(row, state);
        bytes += row.len();
        tokens += found.len();
        state = next;
    }
    (bytes, tokens)
}

/// Builds a page and fails when lexing it gives at most `floor` tokens.
fn page(language: &Language, rows: Vec<String>, floor: usize) -> Page {
    let page = Page {
        lexer: Lexer::new(language),
        rows,
    };
    let (bytes, tokens) = lex_page(&page);
    assert!(tokens > floor, "fixture has only {tokens} tokens");
    let total: usize = page.rows.iter().map(String::len).sum();
    assert_eq!(bytes, total, "every row must be lexed");
    page
}

/// Returns the built-in a2l language, panicking when the builtin list loses it.
fn a2l_language() -> Language {
    navo_syntax::builtin()
        .into_iter()
        .find(|language| language.name == "a2l")
        .unwrap()
}

/// Builds 120 A2L block rows with keywords, a string, a number and a comment.
pub fn a2l_page() -> Page {
    let language = a2l_language();
    let rows = (0..120)
        .map(|index| {
            format!(
                "    /begin MEASUREMENT EngSpeed_{index} \"engine speed {index}\" UWORD ECU_ADDRESS 0x{index:04X} /* row {index} */"
            )
        })
        .collect();
    page(&language, rows, 500)
}

const GROUPS: usize = 8;
const WORDS: usize = 500;

/// Builds the dense UDL text: 8 keyword groups of 500 generated words each.
fn dense_udl() -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" ?>\n<NotepadPlus>\n  <UserLang name=\"dense\" ext=\"dense\">\n    <Settings><Global caseIgnored=\"no\" /></Settings>\n    <KeywordLists>\n",
    );
    for group in 0..GROUPS {
        xml.push_str(&format!("      <Keywords name=\"Keywords{}\">", group + 1));
        for word in 0..WORDS {
            xml.push_str(&format!("w{group}_{word:03} "));
        }
        xml.push_str("</Keywords>\n");
    }
    xml.push_str("    </KeywordLists>\n  </UserLang>\n</NotepadPlus>\n");
    xml
}

/// Loads the dense UDL through the public importer.
fn dense_language() -> Language {
    from_udl(&dense_udl()).unwrap().into_iter().next().unwrap()
}

/// Repeats the generated words until there are 120 rows of 4096 ASCII bytes.
fn dense_rows() -> Vec<String> {
    let mut text = String::new();
    while text.len() < 120 * 4096 {
        for group in 0..GROUPS {
            for word in 0..WORDS {
                text.push_str(&format!("w{group}_{word:03} "));
            }
        }
    }
    (0..120)
        .map(|index| text[index * 4096..(index + 1) * 4096].to_owned())
        .collect()
}

/// Builds 120 dense UDL rows of 4096 bytes, all keyword tokens.
pub fn udl_dense() -> Page {
    page(&dense_language(), dense_rows(), 20_000)
}

/// Builds one row with 20 000 keywords and asserts the cap keeps exactly 4096 tokens.
pub fn udl_tokens() -> Page {
    let page = Page {
        lexer: Lexer::new(&dense_language()),
        rows: vec!["w0_000 ".repeat(20_000)],
    };
    let (_, tokens) = lex_page(&page);
    assert_eq!(tokens, MAX_TOKENS, "the cap must keep {MAX_TOKENS} tokens");
    page
}
