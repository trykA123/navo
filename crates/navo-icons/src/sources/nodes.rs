//! Builds icon SVGs from the Lucide and Tabler node JSON layouts.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::{SourceError, Style, clean, parse_json, push_words};
use crate::pack::IconDraft;

/// Builds every icon in the node map, lower-casing names and optionally joining a tags file.
pub(super) fn build(
    files: &BTreeMap<String, Vec<u8>>,
    file: &str,
    style: Style,
    tags_file: Option<&str>,
    grid: u16,
) -> Result<Vec<IconDraft>, SourceError> {
    let nodes = files
        .get(file)
        .ok_or_else(|| SourceError::Missing(file.to_owned()))?;
    let nodes = parse_json(nodes)?;
    let Value::Object(nodes) = nodes else {
        return Err(SourceError::Json("node map is not an object".to_owned()));
    };
    let tags = tags_file.map(|path| read_tags(files, path)).transpose()?;
    let mut icons = Vec::with_capacity(nodes.len());
    for (name, shape) in &nodes {
        let name = name.to_ascii_lowercase();
        let svg = document(&name, shape, style, grid)?;
        let tags = tags
            .as_ref()
            .and_then(|map| map.get(&name))
            .cloned()
            .unwrap_or_default();
        icons.push(IconDraft {
            name: name.clone(),
            svg: clean(&name, &svg)?,
            tags,
            brand: None,
        });
    }
    Ok(icons)
}

/// Reads either the Lucide array form or the Tabler `{ "tags": [...] }` form.
fn read_tags(
    files: &BTreeMap<String, Vec<u8>>,
    path: &str,
) -> Result<BTreeMap<String, Vec<String>>, SourceError> {
    let bytes = files
        .get(path)
        .ok_or_else(|| SourceError::Missing(path.to_owned()))?;
    let value = parse_json(bytes)?;
    let Value::Object(entries) = value else {
        return Err(SourceError::Json("tags file is not an object".to_owned()));
    };
    let mut tags = BTreeMap::new();
    for (name, entry) in &entries {
        let items = match entry {
            Value::Array(items) => Some(items),
            Value::Object(object) => object.get("tags").and_then(Value::as_array),
            _ => None,
        };
        let mut words = Vec::new();
        for item in items.into_iter().flatten() {
            if let Some(text) = item.as_str() {
                push_words(text, &mut words);
            }
        }
        tags.insert(name.to_ascii_lowercase(), words);
    }
    Ok(tags)
}

/// Wraps the node list in the style's SVG shell and serialises its escaped elements.
fn document(name: &str, shape: &Value, style: Style, grid: u16) -> Result<Vec<u8>, SourceError> {
    let Value::Array(items) = shape else {
        return Err(SourceError::Json(format!("icon `{name}` is not an array")));
    };
    let ink = "#000";
    let mut out = match style {
        Style::Stroke => format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {grid} {grid}" fill="none" stroke="{ink}" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">"##
        ),
        Style::Fill => format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {grid} {grid}" fill="{ink}">"##
        ),
    };
    for item in items {
        if let Some(element) = element(item) {
            out.push_str(&element);
        }
    }
    out.push_str("</svg>");
    Ok(out.into_bytes())
}

/// Serialises one `[tag, attrs]` pair, refusing invalid names and nested values.
fn element(item: &Value) -> Option<String> {
    let Value::Array(pair) = item else {
        return None;
    };
    let tag = pair.first().and_then(Value::as_str)?;
    if !valid_name(tag) {
        return None;
    }
    let mut out = String::from("<");
    out.push_str(tag);
    if let Some(Value::Object(attrs)) = pair.get(1) {
        push_attrs(attrs, &mut out);
    }
    out.push_str("/>");
    Some(out)
}

/// Appends the escaped attributes with valid names to one element.
fn push_attrs(attrs: &Map<String, Value>, out: &mut String) {
    for (key, value) in attrs {
        if !valid_name(key) {
            continue;
        }
        let Some(text) = scalar_text(value) else {
            continue;
        };
        out.push(' ');
        out.push_str(key);
        out.push_str("=\"");
        escape_attr(&text, out);
        out.push('"');
    }
}

/// Says whether `name` is a safe XML identifier made of ASCII letters, digits, `-` and `_`.
fn valid_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|byte| byte.is_ascii_alphanumeric() || byte == '-' || byte == '_')
}

/// Renders a scalar attribute value, refusing nested arrays and objects.
fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

/// Escapes `&`, `"`, `<` and `>` for use inside a double-quoted attribute.
fn escape_attr(text: &str, out: &mut String) {
    for byte in text.chars() {
        match byte {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(byte),
        }
    }
}
