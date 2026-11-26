//! Builds Simple Icons drafts, dropping every entry that carries a licence key.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::{SourceError, clean, parse_json, push_words};
use crate::pack::IconDraft;

/// The metadata path in the npm package.
const DATA: &str = "package/data/simple-icons.json";
/// The metadata path when the archive has no `package/` prefix.
const DATA_FALLBACK: &str = "data/simple-icons.json";

/// Builds the surviving icons, resolving each slug, colour, tags and SVG file.
pub(super) fn build(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<IconDraft>, SourceError> {
    let bytes = files
        .get(DATA)
        .or_else(|| files.get(DATA_FALLBACK))
        .ok_or_else(|| SourceError::Missing(DATA.to_owned()))?;
    let value = parse_json(bytes)?;
    let entries = match &value {
        Value::Array(items) => items.as_slice(),
        Value::Object(object) => object
            .get("icons")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                SourceError::Json("simple-icons object has no `icons` array".to_owned())
            })?
            .as_slice(),
        _ => {
            return Err(SourceError::Json(
                "simple-icons JSON is not an array or object".to_owned(),
            ));
        }
    };
    let mut icons = Vec::new();
    for entry in entries {
        let Value::Object(entry) = entry else {
            continue;
        };
        if entry.contains_key("license") {
            continue;
        }
        let title = entry
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let slug = entry
            .get("slug")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| slugify(title));
        if slug.is_empty() {
            continue;
        }
        let svg = files
            .get(&format!("package/icons/{slug}.svg"))
            .or_else(|| files.get(&format!("icons/{slug}.svg")))
            .ok_or_else(|| SourceError::Missing(format!("package/icons/{slug}.svg")))?;
        let name = slug.to_ascii_lowercase();
        icons.push(IconDraft {
            name: name.clone(),
            svg: clean(&name, svg)?,
            tags: tags(entry, title),
            brand: brand(entry.get("hex")),
        });
    }
    Ok(icons)
}

/// Collects words from the title and every `aliases.aka` string.
fn tags(entry: &Map<String, Value>, title: &str) -> Vec<String> {
    let mut tags = Vec::new();
    push_words(title, &mut tags);
    if let Some(aka) = entry
        .get("aliases")
        .and_then(Value::as_object)
        .and_then(|aliases| aliases.get("aka"))
        .and_then(Value::as_array)
    {
        for alias in aka {
            if let Some(text) = alias.as_str() {
                push_words(text, &mut tags);
            }
        }
    }
    tags
}

/// Parses a six-digit hex colour with or without a leading `#`.
fn brand(value: Option<&Value>) -> Option<[u8; 3]> {
    let text = value?.as_str()?;
    let hex = text.strip_prefix('#').unwrap_or(text);
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let pair = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    Some([pair(0)?, pair(2)?, pair(4)?])
}

/// Derives a lower-case dash-separated slug from a title.
fn slugify(title: &str) -> String {
    let mut slug = String::new();
    for byte in title.chars() {
        if byte.is_ascii_alphanumeric() {
            slug.push(byte.to_ascii_lowercase());
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_end_matches('-').to_owned()
}
