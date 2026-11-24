//! JSON syntax lint: turns serde_json's first error into a ranged Diagnostic.

use super::{Diagnostic, Severity, position};

/// Returns the first JSON syntax error in `text`, or None when it parses.
pub(super) fn lint_json(text: &str) -> Option<Diagnostic> {
    let (source, bom) = match text.strip_prefix('\u{feff}') {
        Some(rest) => (rest, '\u{feff}'.len_utf8()),
        None => (text, 0),
    };
    let error = serde_json::from_str::<serde::de::IgnoredAny>(source).err()?;
    let start = line_start(source, error.line());
    // Column 0 marks the point before the line, which is its preceding newline.
    let offset = if error.column() == 0 {
        start.saturating_sub(1)
    } else {
        (start + error.column() - 1).min(source.len())
    } + bom;
    let end = text
        .get(offset..)
        .and_then(|rest| rest.chars().next())
        .map_or(offset, |ch| offset + ch.len_utf8());
    let (line, col) = position(text, offset);
    Some(Diagnostic {
        range: offset..end,
        line,
        col,
        severity: Severity::Error,
        message: message(&error),
    })
}

/// Byte offset where 1-based `line` starts; past the text it returns the length.
fn line_start(text: &str, line: usize) -> usize {
    let mut remaining = line.saturating_sub(1);
    if remaining == 0 {
        return 0;
    }
    for (at, byte) in text.bytes().enumerate() {
        if byte == b'\n' {
            remaining -= 1;
            if remaining == 0 {
                return at + 1;
            }
        }
    }
    text.len()
}

/// serde_json's error text without its " at line L column C" suffix.
fn message(error: &serde_json::Error) -> String {
    let text = error.to_string();
    let suffix = format!(" at line {} column {}", error.line(), error.column());
    text.strip_suffix(&suffix).unwrap_or(&text).to_owned()
}
