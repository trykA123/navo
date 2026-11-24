//! Pretty-print helpers for JSON, TOML and XML texts in the editor (F3).
//! Returns the first lint error unchanged when the text does not parse.

mod json;
mod toml;
mod xml;

#[cfg(test)]
mod tests;

use crate::lint::{Diagnostic, LintKind, Severity, lint};

/// Returns a pretty-printed `text` for `kind`; fails with the first syntax error.
pub fn format(kind: LintKind, text: &str) -> Result<String, Diagnostic> {
    if let Some(error) = lint(kind, text)
        .into_iter()
        .find(|diagnostic| diagnostic.severity == Severity::Error)
    {
        return Err(error);
    }
    match kind {
        LintKind::Json => Ok(json::format_json(text)),
        LintKind::Toml => Ok(toml::format_toml(text)),
        LintKind::Xml => Ok(xml::format_xml(text).unwrap_or_else(|| text.to_owned())),
        LintKind::Yaml | LintKind::Csv => Err(unsupported()),
    }
}

/// The error for a kind the formatter does not support.
fn unsupported() -> Diagnostic {
    Diagnostic {
        range: 0..0,
        line: 1,
        col: 1,
        severity: Severity::Error,
        message: "formatting is not available for this kind".to_owned(),
    }
}

/// Applies the input's line ending, keeps a leading BOM and exactly one final newline.
fn finish(body: &str, text: &str, bom: usize) -> String {
    let normalized = body.replace("\r\n", "\n");
    let trimmed = normalized.trim_end_matches(['\r', '\n']);
    let ending = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = String::with_capacity(trimmed.len() + bom + ending.len());
    if bom > 0 {
        out.push('\u{feff}');
    }
    if ending == "\r\n" {
        out.push_str(&trimmed.replace('\n', "\r\n"));
    } else {
        out.push_str(trimmed);
    }
    out.push_str(ending);
    out
}
