//! YAML syntax lint: turns saphyr's first scan error into a ranged Diagnostic.

use saphyr_parser::Parser;

use super::{Diagnostic, Severity, char_range, line_end, line_start, position, split_bom};

/// Returns the first YAML syntax error in `text`, or None when it parses.
pub(super) fn lint_yaml(text: &str) -> Option<Diagnostic> {
    let (source, bom) = split_bom(text);
    let mut parser = Parser::new_from_str(source);
    while let Some(next) = parser.next_event() {
        let error = match next {
            Ok(_) => continue,
            Err(error) => error,
        };
        let marker = error.marker();
        let offset = offset_of(source, marker.line(), marker.col()) + bom;
        let (line, col) = position(text, offset);
        return Some(Diagnostic {
            range: char_range(text, offset),
            line,
            col,
            severity: Severity::Error,
            message: error.info().trim().to_owned(),
        });
    }
    None
}

/// Byte offset of the 1-based `line` and 0-based character `col`, clamped to the line end.
fn offset_of(text: &str, line: usize, col: usize) -> usize {
    let start = line_start(text, line).min(text.len());
    let end = line_end(text, start);
    let mut offset = start;
    for ch in text.get(start..end).unwrap_or("").chars().take(col) {
        offset += ch.len_utf8();
    }
    offset
}
