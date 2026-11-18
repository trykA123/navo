//! Builds every benchmark page fixture, so the setup assertions run without Valgrind.

#[path = "../benches/support/mod.rs"]
mod support;

use sidewise_core::lines::{LineLexer, LineState, MAX_PAGE_SPANS};

/// Counts the spans of each row of a page, carrying state like the page lexer does.
fn row_spans(page: &support::Page) -> Vec<usize> {
    let mut state = LineState::default();
    page.rows
        .iter()
        .map(|row| {
            let (found, next) = page.lexer.lex(row, state);
            state = next;
            found.len()
        })
        .collect()
}

#[test]
fn every_fixture_builds_and_passes_its_setup_assertions() {
    support::arxml_typical();
    support::xml_dense(120, false);
    support::xml_dense(300, true);
    support::json_dense();
    support::log_typical();
    support::log_dense();
    support::log_1mib();
    support::line_over_max();
}

#[test]
fn line_over_max_is_rejected_by_the_editor_cap() {
    let line = support::line_over_max();
    assert_eq!(support::lex_line(&line), (0, 0));
    assert!(line.len() > sidewise_core::lines::MAX_LINE);
}

#[test]
fn json_dense_rows_stay_under_the_page_span_cap() {
    let spans = row_spans(&support::json_dense());
    let total: usize = spans.iter().sum();
    assert_eq!(spans.len(), 120);
    assert!(total < MAX_PAGE_SPANS, "{total} spans over 120 rows");
}
