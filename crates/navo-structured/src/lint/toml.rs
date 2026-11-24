//! TOML syntax lint: turns toml_edit's first error into a ranged Diagnostic.

use super::{Diagnostic, Severity, position};

/// Returns the first TOML syntax error in `text`, or None when it parses.
pub(super) fn lint_toml(text: &str) -> Option<Diagnostic> {
    let error = match text.parse::<toml_edit::DocumentMut>() {
        Ok(_) => return None,
        Err(error) => error,
    };
    let mut range = error.span().unwrap_or(0..0);
    if range.is_empty()
        && range.start < text.len()
        && let Some(ch) = text.get(range.start..).and_then(|rest| rest.chars().next())
    {
        range = range.start..range.start + ch.len_utf8();
    }
    let (line, col) = position(text, range.start);
    Some(Diagnostic {
        range,
        line,
        col,
        severity: Severity::Error,
        message: error.message().trim().to_owned(),
    })
}
