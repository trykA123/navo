//! TOML formatter: keeps every comment and value, stripping only trailing whitespace.

use crate::lint::split_bom;

/// Returns `text` with line-end spaces removed and line endings unified; no content dropped.
pub(super) fn format_toml(text: &str) -> String {
    let (source, bom) = split_bom(text);
    let mut cleaned = String::with_capacity(source.len());
    for (index, line) in source.split('\n').enumerate() {
        if index > 0 {
            cleaned.push('\n');
        }
        cleaned.push_str(line.trim_end_matches([' ', '\t', '\r']));
    }
    super::finish(&cleaned, text, bom)
}
