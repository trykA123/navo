//! Strict allowlist XML pre-parser that neutralises SVG amplification bombs before `usvg`.

use std::collections::{BTreeSet, HashSet};
use std::fmt::Write as _;

use roxmltree::{Document, Node, ParsingOptions};

use super::{Removed, SVG_CAP, SvgError};

/// Maximum XML element nesting depth permitted before rejecting the document.
const MAX_DEPTH: usize = 64;
/// Maximum total XML elements inspected during pre-parsing.
const MAX_XML_ELEMENTS: usize = 25_000;
/// Maximum total XML nodes (elements + text/comments) accepted by `roxmltree`.
const MAX_XML_NODES: u32 = 60_000;

const ALLOWED_ELEMENTS: &[&str] = &[
    "svg",
    "g",
    "defs",
    "path",
    "circle",
    "rect",
    "line",
    "polyline",
    "polygon",
    "ellipse",
    "linearGradient",
    "radialGradient",
    "stop",
];

const PASS_THROUGH_ATTRS: &[&str] = &[
    "viewBox",
    "width",
    "height",
    "x",
    "y",
    "x1",
    "y1",
    "x2",
    "y2",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "d",
    "points",
    "transform",
    "fill-rule",
    "fill-opacity",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-dasharray",
    "stroke-dashoffset",
    "stroke-opacity",
    "opacity",
    "offset",
    "stop-opacity",
    "gradientUnits",
    "gradientTransform",
];

/// Sanitized XML subset ready for `usvg`, plus any features stripped during pre-parsing.
pub(super) struct Preparsed {
    pub(super) xml: String,
    pub(super) removed: BTreeSet<Removed>,
}

struct IdIndex {
    gradient_ids: HashSet<String>,
    pattern_ids: HashSet<String>,
}

/// Applies raw byte gates and rewrites untrusted SVG bytes into a strictly allowlisted XML subset.
pub(super) fn preparse(bytes: &[u8]) -> Result<Preparsed, SvgError> {
    check_raw_gates(bytes)?;
    let text = std::str::from_utf8(bytes).map_err(|_| SvgError::NotUtf8)?;
    if contains_doctype_or_entity(text.as_bytes()) {
        return Err(SvgError::Doctype);
    }

    let opts = ParsingOptions {
        allow_dtd: false,
        nodes_limit: MAX_XML_NODES,
        ..ParsingOptions::default()
    };
    let doc = Document::parse_with_options(text, opts).map_err(map_xml_error)?;
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return Err(SvgError::Parse("root element is not <svg>".to_owned()));
    }

    let ids = index_and_validate(root)?;
    let mut removed = BTreeSet::new();
    let mut emitted_gradients = HashSet::new();
    let mut xml = String::with_capacity(text.len().min(64 * 1024));
    emit_element(root, &ids, &mut emitted_gradients, &mut removed, &mut xml);
    Ok(Preparsed { xml, removed })
}

fn check_raw_gates(bytes: &[u8]) -> Result<(), SvgError> {
    if bytes.len() > SVG_CAP {
        return Err(SvgError::TooLarge);
    }
    if bytes.starts_with(&[0x1F, 0x8B]) {
        return Err(SvgError::Compressed);
    }
    Ok(())
}

fn contains_doctype_or_entity(bytes: &[u8]) -> bool {
    const DOCTYPE: &[u8] = b"<!doctype";
    const ENTITY: &[u8] = b"<!entity";
    bytes
        .windows(DOCTYPE.len())
        .any(|w| w.eq_ignore_ascii_case(DOCTYPE))
        || bytes
            .windows(ENTITY.len())
            .any(|w| w.eq_ignore_ascii_case(ENTITY))
}

fn map_xml_error(err: roxmltree::Error) -> SvgError {
    match err {
        roxmltree::Error::DtdDetected => SvgError::Doctype,
        roxmltree::Error::NodesLimitReached => SvgError::TooManyPaths,
        other => SvgError::Parse(other.to_string()),
    }
}

/// Iteratively enforces `MAX_DEPTH` and `MAX_XML_ELEMENTS` while indexing gradient and pattern IDs.
fn index_and_validate(root: Node<'_, '_>) -> Result<IdIndex, SvgError> {
    let mut gradient_ids = HashSet::new();
    let mut pattern_ids = HashSet::new();
    let mut elements = 0usize;
    let mut stack = vec![(root, 1usize, true)];

    while let Some((node, depth, in_allowed_tree)) = stack.pop() {
        if depth > MAX_DEPTH {
            return Err(SvgError::Parse(
                "maximum SVG nesting depth exceeded".to_owned(),
            ));
        }
        elements += 1;
        if elements > MAX_XML_ELEMENTS {
            return Err(SvgError::TooManyPaths);
        }

        let name = node.tag_name().name();
        let allowed = in_allowed_tree && is_allowed_element(name);
        if name == "pattern" {
            if let Some(id) = clean_id(node) {
                pattern_ids.insert(id);
            }
        } else if allowed
            && is_gradient(name)
            && let Some(id) = clean_id(node)
        {
            gradient_ids.insert(id);
        }

        for child in node.children().filter(Node::is_element) {
            stack.push((child, depth + 1, allowed));
        }
    }

    Ok(IdIndex {
        gradient_ids,
        pattern_ids,
    })
}

fn emit_element(
    node: Node<'_, '_>,
    ids: &IdIndex,
    emitted_gradients: &mut HashSet<String>,
    removed: &mut BTreeSet<Removed>,
    out: &mut String,
) {
    let name = node.tag_name().name();
    if let Some(dropped) = removed_for_element(name) {
        removed.insert(dropped);
        return;
    }
    if !is_allowed_element(name) {
        return;
    }

    let grad_id = if is_gradient(name) {
        let Some(id) = clean_id(node) else {
            return;
        };
        if !emitted_gradients.insert(id.clone()) {
            return;
        }
        Some(id)
    } else {
        None
    };

    out.push('<');
    out.push_str(name);
    if name == "svg" {
        out.push_str(" xmlns=\"http://www.w3.org/2000/svg\"");
    }
    if let Some(id) = grad_id {
        write_attr(out, "id", &id);
    }
    emit_attributes(node, ids, removed, out);

    let mut has_element_child = false;
    for child in node.children().filter(Node::is_element) {
        if !has_element_child {
            out.push('>');
            has_element_child = true;
        }
        emit_element(child, ids, emitted_gradients, removed, out);
    }
    if has_element_child {
        let _ = write!(out, "</{name}>");
    } else {
        out.push_str("/>");
    }
}

fn emit_attributes(
    node: Node<'_, '_>,
    ids: &IdIndex,
    removed: &mut BTreeSet<Removed>,
    out: &mut String,
) {
    for attr in node.attributes() {
        if attr.namespace().is_some() {
            continue;
        }
        let name = attr.name();
        let value = attr.value().trim();
        match name {
            "filter" => record_if_active(value, Removed::Filter, removed),
            "mask" => record_if_active(value, Removed::Mask, removed),
            "clip-path" => record_if_active(value, Removed::ClipPath, removed),
            "fill" => emit_paint_attr(out, "fill", value, true, ids, removed),
            "stroke" => emit_paint_attr(out, "stroke", value, false, ids, removed),
            "color" | "stop-color" => emit_color_attr(out, name, value),
            other if PASS_THROUGH_ATTRS.contains(&other) => write_attr(out, other, value),
            _ => {}
        }
    }
}

fn record_if_active(value: &str, feature: Removed, removed: &mut BTreeSet<Removed>) {
    if !value.is_empty() && !value.eq_ignore_ascii_case("none") {
        removed.insert(feature);
    }
}

fn emit_paint_attr(
    out: &mut String,
    name: &str,
    value: &str,
    none_on_strip: bool,
    ids: &IdIndex,
    removed: &mut BTreeSet<Removed>,
) {
    if is_context_paint(value) {
        if none_on_strip {
            write_attr(out, name, "none");
        }
        return;
    }
    if starts_with_url(value) {
        if let Some(target) = parse_url_fragment(value) {
            if ids.gradient_ids.contains(target) {
                removed.insert(Removed::Gradient);
                let mut canonical = String::with_capacity(target.len() + 6);
                canonical.push_str("url(#");
                canonical.push_str(target);
                canonical.push(')');
                write_attr(out, name, &canonical);
                return;
            }
            if ids.pattern_ids.contains(target) {
                removed.insert(Removed::Pattern);
            }
        }
        if none_on_strip {
            write_attr(out, name, "none");
        }
        return;
    }
    write_attr(out, name, value);
}

fn emit_color_attr(out: &mut String, name: &str, value: &str) {
    if is_context_paint(value) || contains_url(value) {
        return;
    }
    write_attr(out, name, value);
}

fn write_attr(out: &mut String, name: &str, value: &str) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            other => out.push(other),
        }
    }
    out.push('"');
}

fn clean_id(node: Node<'_, '_>) -> Option<String> {
    let raw = node.attribute("id")?.trim();
    if raw.is_empty() || raw.len() > 128 {
        return None;
    }
    let valid = raw
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'));
    valid.then(|| raw.to_owned())
}

fn parse_url_fragment(value: &str) -> Option<&str> {
    let open = value.find('(')?;
    let close = value[open + 1..].find(')')?;
    let inner = value[open + 1..open + 1 + close]
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .trim();
    let fragment = inner.strip_prefix('#')?.trim();
    (!fragment.is_empty()).then_some(fragment)
}

fn starts_with_url(value: &str) -> bool {
    value.len() >= 4 && value.as_bytes()[..4].eq_ignore_ascii_case(b"url(")
}

fn contains_url(value: &str) -> bool {
    value
        .as_bytes()
        .windows(4)
        .any(|w| w.eq_ignore_ascii_case(b"url("))
}

fn is_context_paint(value: &str) -> bool {
    value.eq_ignore_ascii_case("context-fill") || value.eq_ignore_ascii_case("context-stroke")
}

fn is_allowed_element(name: &str) -> bool {
    ALLOWED_ELEMENTS.contains(&name)
}

fn is_gradient(name: &str) -> bool {
    matches!(name, "linearGradient" | "radialGradient")
}

fn removed_for_element(name: &str) -> Option<Removed> {
    match name {
        "text" | "tspan" | "textPath" => Some(Removed::Text),
        "image" => Some(Removed::Image),
        "filter" => Some(Removed::Filter),
        "mask" => Some(Removed::Mask),
        "clipPath" => Some(Removed::ClipPath),
        "pattern" => Some(Removed::Pattern),
        _ => None,
    }
}
