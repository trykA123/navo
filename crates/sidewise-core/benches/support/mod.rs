//! Page fixtures for the line lexer benchmarks, also run as a plain test (tests/bench_fixtures.rs).

use sidewise_core::lines::{
    LineLanguage, LineLexer, LineState, MAX_LINE, MAX_PAGE_BYTES, MAX_PAGE_SPANS,
};

/// Holds the rows and lexer for one page benchmark.
pub struct Page {
    pub lexer: LineLanguage,
    pub rows: Vec<String>,
}

/// Lexes page rows with carried state and returns the byte and span counts.
pub fn lex_page(page: &Page) -> (usize, usize) {
    let mut state = LineState::default();
    let mut bytes = 0;
    let mut spans = 0;
    for row in &page.rows {
        if bytes + row.len() > MAX_PAGE_BYTES || spans >= MAX_PAGE_SPANS {
            break;
        }
        let (found, next) = page.lexer.lex(row, state);
        bytes += row.len();
        spans += found.len();
        state = next;
        if row.ends_with('\n') {
            state = page.lexer.line_end(state);
        }
    }
    (bytes, spans)
}

/// Builds and validates a page, failing if it does not exercise enough spans.
pub fn page(lexer: LineLanguage, rows: Vec<String>, floor: usize, capped: bool) -> Page {
    let page = Page { lexer, rows };
    let (bytes, spans) = lex_page(&page);
    assert!(spans > floor, "fixture has only {spans} spans");
    if capped {
        assert_eq!(bytes, MAX_PAGE_BYTES, "page must stop at the byte cap");
    } else {
        let total: usize = page.rows.iter().map(String::len).sum();
        assert_eq!(bytes, total, "every row must be lexed");
    }
    page
}

/// Builds 120 ordinary Log lines with levels, dates, paths, numbers and strings.
pub fn log_typical() -> Page {
    let row = "2026-11-18T09:15:02.123Z ERROR C:\\Work\\Pr\u{fc}fung\\ecu.arxml code 0x1F count 42 msg \"started ok\" filler words go here\n";
    page(LineLanguage::Log, vec![row.to_owned(); 120], 400, false)
}

/// One repeated dense Log line: level, time, hex, string and path.
const DENSE: &str =
    "ERROR 09:15:02 0x1F \"quoted text\" C:\\a\\b abcdefghijklmnopqrstuvwxyz plain words pad more ";

/// Builds `rows` rows of exactly `row` bytes from the repeated dense Log text.
fn dense_rows(rows: usize, row: usize) -> Vec<String> {
    let input = DENSE.repeat((rows * row).div_ceil(DENSE.len()));
    (0..rows)
        .map(|index| input[index * row..(index + 1) * row].to_owned())
        .collect()
}

/// Builds 120 dense Log rows of 4096 bytes each.
pub fn log_dense() -> Page {
    page(LineLanguage::Log, dense_rows(120, 4096), 20_000, false)
}

/// Builds a 1 MiB page of dense Log rows for a catch-up run.
pub fn log_1mib() -> Page {
    page(LineLanguage::Log, dense_rows(256, 4096), 50_000, false)
}

/// Lexes one editor line, or returns zero when it is longer than MAX_LINE.
pub fn lex_line(line: &str) -> (usize, usize) {
    if line.len() > MAX_LINE {
        return (0, 0);
    }
    let (spans, _) = LineLanguage::Log.lex(line, LineState::default());
    (line.len(), spans.len())
}

/// Builds one 1 MiB Log line and asserts the editor line cap rejects it.
pub fn line_over_max() -> String {
    let text = DENSE.repeat(MAX_PAGE_BYTES.div_ceil(DENSE.len()));
    let line = text[..MAX_PAGE_BYTES].to_owned();
    assert_eq!(
        lex_line(&line),
        (0, 0),
        "a line over MAX_LINE must not be lexed"
    );
    line
}

/// Builds 120 ordinary ARXML lines with several attributes each.
pub fn arxml_typical() -> Page {
    let row = "<ELEMENT NAME=\"EngineSpeed\" ID=\"12345\" TYPE=\"VALUE\" UNIT=\"rpm\">Speed value</ELEMENT>\n";
    page(LineLanguage::Xml, vec![row.to_owned(); 120], 600, false)
}

/// Builds a minified XML page from fixed-size ASCII chunks.
pub fn xml_dense(rows: usize, capped: bool) -> Page {
    let text = format!("<a x=\"v\">{}</a>", " ".repeat(133));
    let input = text.repeat((rows * 4096).div_ceil(text.len()));
    let chunks = (0..rows)
        .map(|index| input[index * 4096..(index + 1) * 4096].to_owned())
        .collect();
    page(LineLanguage::Xml, chunks, 30_000, capped)
}

/// Builds a JSON page from fixed-size ASCII chunks of padded records.
///
/// The padding keeps all 120 rows under the page span cap, so every row is lexed.
pub fn json_dense() -> Page {
    let text = format!(
        "{{\"value\":123,\"name\":\"{}\"}},{}",
        "a".repeat(38),
        " ".repeat(27)
    );
    let input = text.repeat((120 * 4096_usize).div_ceil(text.len()));
    let chunks = (0..120)
        .map(|index| input[index * 4096..(index + 1) * 4096].to_owned())
        .collect();
    page(LineLanguage::Json, chunks, 30_000, false)
}
