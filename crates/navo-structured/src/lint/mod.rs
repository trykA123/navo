//! Syntax lint for JSON, TOML, YAML, XML and CSV texts.
//! Reports parse problems as byte ranges plus 1-based line and column for the editor (E5).

mod csv_lint;
mod json;
mod toml;
mod xml;
mod yaml;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintKind {
    Json,
    Toml,
    Yaml,
    Xml,
    Csv,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// One problem in a text: `range` is a byte range into the text given to `lint`;
/// `line` and `col` are 1-based, `col` counts characters (not bytes) from the line start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: std::ops::Range<usize>,
    pub line: u32,
    pub col: u32,
    pub severity: Severity,
    pub message: String,
}

/// Returns the syntax problems of `text` for `kind`; empty when the text parses.
pub fn lint(kind: LintKind, text: &str) -> Vec<Diagnostic> {
    match kind {
        LintKind::Json => json::lint_json(text).into_iter().collect(),
        LintKind::Toml => toml::lint_toml(text).into_iter().collect(),
        LintKind::Yaml => yaml::lint_yaml(text).into_iter().collect(),
        LintKind::Xml => xml::lint_xml(text).into_iter().collect(),
        LintKind::Csv => csv_lint::lint_csv(text),
    }
}

/// 1-based line and character column of byte `offset` in `text` (offset clamped to len,
/// then moved back to a char boundary); a leading U+FEFF is not counted as a column.
fn position(text: &str, offset: usize) -> (u32, u32) {
    let mut offset = offset.min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let before = &text[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1;
    let line_start = before.rfind('\n').map_or(0, |at| at + 1);
    let count_from = if line_start == 0 && text.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        line_start
    };
    let col = if offset > count_from {
        text.get(count_from..offset)
            .map_or(1, |part| part.chars().count() as u32 + 1)
    } else {
        1
    };
    (line, col)
}

/// Splits a leading U+FEFF from `text`; returns the rest and the BOM's byte length.
fn split_bom(text: &str) -> (&str, usize) {
    match text.strip_prefix('\u{feff}') {
        Some(rest) => (rest, '\u{feff}'.len_utf8()),
        None => (text, 0),
    }
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

/// Byte offset just past the line starting at `start`, without its newline or `\r`.
fn line_end(text: &str, start: usize) -> usize {
    let end = text
        .get(start..)
        .and_then(|rest| rest.find('\n'))
        .map_or(text.len(), |at| start + at);
    if text.get(..end).is_some_and(|before| before.ends_with('\r')) {
        end - 1
    } else {
        end
    }
}

/// Range of the single char at byte `offset`, empty at or past the end of `text`.
fn char_range(text: &str, offset: usize) -> std::ops::Range<usize> {
    let offset = offset.min(text.len());
    text.get(offset..)
        .and_then(|rest| rest.chars().next())
        .map_or(offset..offset, |ch| offset..offset + ch.len_utf8())
}
