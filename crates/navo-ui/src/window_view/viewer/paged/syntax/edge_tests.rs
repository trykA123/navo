//! Edge-case tests for the paged syntax fill: the cold-resync head and the page byte cap.

use navo_core::paging::row_at;

use sidewise_kit::languages::lines::{LineLanguage, LineLexer, LineState, Span};

use super::lex_page;
use super::tests::{page_of, roles, row, xml};

#[test]
fn a_cold_head_keeps_a_separator_between_logical_lines() {
    let text = "a\n--\r\n>\n<A/>\n";
    let top = row_at(text.as_bytes(), 1).expect("row").start;
    let cold = page_of(text, top, Some(xml()));
    assert!(
        cold.iter()
            .all(|line| line.spans.iter().all(|(_, role)| *role != "comment")),
        "{:?}",
        cold.iter().map(roles).collect::<Vec<_>>()
    );
}

/// Records which lex method each chunk used; marks nothing.
#[derive(Default)]
struct Which(std::sync::Mutex<Vec<(String, bool)>>);

impl LineLexer for Which {
    fn name(&self) -> &str {
        "which"
    }
    fn lex(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        self.0.lock().expect("log").push((chunk.to_owned(), false));
        (vec![], state)
    }
    fn lex_last(&self, chunk: &str, state: LineState) -> (Vec<Span>, LineState) {
        self.0.lock().expect("log").push((chunk.to_owned(), true));
        (vec![], state)
    }
}

#[test]
fn rows_that_end_a_line_or_the_file_use_lex_last_and_cut_rows_use_lex() {
    let which = Which::default();
    // Rows: a cut row, the end of its line, a whole line, a last row ending at the file end.
    let mut lines = [
        row(0, "ab", true),
        row(2, "cd", false),
        row(5, "ef", false),
        row(8, "gh", true),
    ];
    lex_page(&which, &mut lines, LineState::default(), 10);
    let calls = which.0.lock().expect("log").clone();
    let expected = [("ab", false), ("cd", true), ("ef", true), ("gh", true)];
    let expected: Vec<_> = expected.map(|(t, l)| (t.to_owned(), l)).into();
    assert_eq!(calls, expected);
}

#[test]
fn a_cut_row_before_the_file_end_still_uses_lex() {
    let which = Which::default();
    let mut lines = [row(0, "ab", true), row(2, "cd", true)];
    lex_page(&which, &mut lines, LineState::default(), 100);
    let calls = which.0.lock().expect("log").clone();
    assert!(calls.iter().all(|(_, last)| !last), "{calls:?}");
}

#[test]
fn a_cold_start_at_a_continuation_row_leaves_it_plain_up_to_the_line_end() {
    // A followed file whose last row (offset 4096) continues the line: `ERROR` ends the file.
    let text = format!("{}ERROR", "x".repeat(4096));
    let log: std::sync::Arc<dyn LineLexer> = std::sync::Arc::new(LineLanguage::Log);
    let page = page_of(&text, 4096, Some(log.clone()));
    assert_eq!(page.len(), 1);
    assert!(page[0].spans.is_empty(), "{:?}", page[0].spans);
    // The same word on a row that starts its line is still coloured.
    let page = page_of("ERROR", 0, Some(log));
    assert!(!page[0].spans.is_empty());
}

#[test]
fn colours_resume_on_the_line_after_a_plain_continuation() {
    let text = format!("{}ERROR\nERROR", "x".repeat(4096));
    let log: std::sync::Arc<dyn LineLexer> = std::sync::Arc::new(LineLanguage::Log);
    let page = page_of(&text, 4096, Some(log));
    assert_eq!(page.len(), 2);
    assert!(page[0].spans.is_empty(), "{:?}", page[0].spans);
    assert!(!page[1].spans.is_empty());
}
