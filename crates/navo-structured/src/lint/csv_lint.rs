//! CSV syntax lint: quote scan plus csv row-width errors as ranged diagnostics.

use super::{Diagnostic, Severity, char_range, line_end, position, split_bom};

/// Returns at most one CSV error and one quote warning, sorted by byte offset.
pub(super) fn lint_csv(text: &str) -> Vec<Diagnostic> {
    let (source, bom) = split_bom(text);
    let (warning, unclosed) = scan_quotes(source, text, bom);
    let mut found: Vec<Diagnostic> = warning.into_iter().collect();
    found.extend(unclosed.or_else(|| parse_rows(source, text, bom)));
    found.sort_by_key(|diagnostic| diagnostic.range.start);
    found
}

/// Scans quotes and returns the first stray-quote warning plus any unclosed-field error.
fn scan_quotes(source: &str, text: &str, bom: usize) -> (Option<Diagnostic>, Option<Diagnostic>) {
    let bytes = source.as_bytes();
    let mut at = 0;
    let mut field_start = true;
    let mut opening: Option<usize> = None;
    let mut warning: Option<Diagnostic> = None;
    while at < bytes.len() {
        let byte = bytes[at];
        if opening.is_some() {
            let escaped = byte == b'"' && bytes.get(at + 1) == Some(&b'"');
            if escaped {
                at += 2;
                continue;
            }
            if byte == b'"' {
                opening = None;
            }
            at += 1;
            continue;
        }
        if field_start && byte == b'"' {
            opening = Some(at);
            field_start = false;
            at += 1;
            continue;
        }
        if byte == b'"' && warning.is_none() {
            let range = bom + at..bom + at + 1;
            let message = "quote inside an unquoted field".to_owned();
            warning = Some(diagnostic(text, range, Severity::Warning, message));
        }
        field_start = byte == b',' || byte == b'\n';
        at += 1;
    }
    let unclosed = opening.map(|open| {
        let range = bom + open..bom + open + 1;
        diagnostic(text, range, Severity::Error, "unclosed quote".to_owned())
    });
    (warning, unclosed)
}

/// Reads byte records and returns the first row-width or parser error.
fn parse_rows(source: &str, text: &str, bom: usize) -> Option<Diagnostic> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(false)
        .from_reader(source.as_bytes());
    for result in reader.byte_records() {
        if let Err(error) = result {
            return Some(row_error(&error, source, text, bom));
        }
    }
    None
}

/// Maps a csv error to a diagnostic on its record line or its byte position.
fn row_error(error: &csv::Error, source: &str, text: &str, bom: usize) -> Diagnostic {
    if let csv::ErrorKind::UnequalLengths {
        pos: Some(pos),
        expected_len,
        len,
    } = error.kind()
    {
        let start = record_start(source, pos.byte() as usize);
        let end = line_end(source, start);
        let message = format!(
            "row {} has {} fields, expected {}",
            pos.line(),
            len,
            expected_len
        );
        return diagnostic(text, bom + start..bom + end, Severity::Error, message);
    }
    let range = error
        .position()
        .map_or(0..0, |pos| char_range(text, bom + pos.byte() as usize));
    diagnostic(
        text,
        range,
        Severity::Error,
        error.to_string().trim().to_owned(),
    )
}

/// Moves a csv byte position past a line break onto the record's first byte.
fn record_start(source: &str, at: usize) -> usize {
    match source.as_bytes().get(at..) {
        Some([b'\r', b'\n', ..]) => at + 2,
        Some([b'\r' | b'\n', ..]) => at + 1,
        _ => at,
    }
}

/// Builds a diagnostic over `range` with its 1-based line and character column.
fn diagnostic(
    text: &str,
    range: std::ops::Range<usize>,
    severity: Severity,
    message: String,
) -> Diagnostic {
    let (line, col) = position(text, range.start);
    Diagnostic {
        range,
        line,
        col,
        severity,
        message,
    }
}
