//! Minimal SVG serializer that emits only `<svg>` and `<path>` elements.

use std::fmt::Write as _;

use resvg::usvg::{FillRule, LineCap, LineJoin, tiny_skia_path};

/// Flattened, transformed path ready to be written as a single `<path .../>` element.
pub(super) struct PathItem {
    pub(super) d: String,
    pub(super) fill: Option<FillItem>,
    pub(super) stroke: Option<StrokeItem>,
}

/// Solid fill specification for a normalised `<path>`.
pub(super) struct FillItem {
    pub(super) rgb: [u8; 3],
    pub(super) opacity: f32,
    pub(super) rule: FillRule,
}

/// Solid stroke specification for a normalised `<path>`.
pub(super) struct StrokeItem {
    pub(super) rgb: [u8; 3],
    pub(super) opacity: f32,
    pub(super) width: f32,
    pub(super) linecap: LineCap,
    pub(super) linejoin: LineJoin,
    pub(super) miterlimit: f32,
    pub(super) dasharray: Vec<f32>,
    pub(super) dashoffset: f32,
}

/// Formats a floating-point coordinate or length with at most 3 decimals, trimming trailing zeros and `-0`.
pub(super) fn number(value: f32) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    let rounded = (f64::from(value) * 1000.0).round() / 1000.0;
    if rounded == 0.0 {
        return "0".to_owned();
    }
    let mut text = format!("{rounded:.3}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text == "-0" || text.is_empty() {
        "0".to_owned()
    } else {
        text
    }
}

/// Encodes a transformed `tiny_skia_path::Path` as absolute `M`/`L`/`Q`/`C`/`Z` commands.
pub(super) fn path_data(path: &tiny_skia_path::Path) -> String {
    let mut out = String::new();
    for seg in path.segments() {
        if !out.is_empty() {
            out.push(' ');
        }
        match seg {
            tiny_skia_path::PathSegment::MoveTo(p) => {
                let _ = write!(out, "M{} {}", number(p.x), number(p.y));
            }
            tiny_skia_path::PathSegment::LineTo(p) => {
                let _ = write!(out, "L{} {}", number(p.x), number(p.y));
            }
            tiny_skia_path::PathSegment::QuadTo(p1, p2) => {
                let _ = write!(
                    out,
                    "Q{} {} {} {}",
                    number(p1.x),
                    number(p1.y),
                    number(p2.x),
                    number(p2.y)
                );
            }
            tiny_skia_path::PathSegment::CubicTo(p1, p2, p3) => {
                let _ = write!(
                    out,
                    "C{} {} {} {} {} {}",
                    number(p1.x),
                    number(p1.y),
                    number(p2.x),
                    number(p2.y),
                    number(p3.x),
                    number(p3.y)
                );
            }
            tiny_skia_path::PathSegment::Close => out.push('Z'),
        }
    }
    out
}

/// Serialises the normalised SVG document using only `<svg>` and `<path>` elements.
pub(super) fn write_svg(width: f32, height: f32, paths: &[PathItem]) -> Vec<u8> {
    let mut out = String::with_capacity(128 + paths.len() * 64);
    let _ = write!(
        out,
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {} {}\">",
        number(width),
        number(height)
    );
    for item in paths {
        write_path(&mut out, item);
    }
    out.push_str("</svg>");
    out.into_bytes()
}

fn write_path(out: &mut String, item: &PathItem) {
    let _ = write!(out, "<path d=\"{}\"", item.d);

    let (shared_opacity, separate_opacity) =
        resolve_opacity(item.fill.as_ref(), item.stroke.as_ref());

    match &item.fill {
        Some(fill) => {
            write_hex(out, "fill", fill.rgb);
            if fill.rule == FillRule::EvenOdd {
                out.push_str(" fill-rule=\"evenodd\"");
            }
            if separate_opacity {
                write_opacity(out, "fill-opacity", fill.opacity);
            }
        }
        None => out.push_str(" fill=\"none\""),
    }

    if let Some(stroke) = &item.stroke {
        write_stroke(out, stroke, separate_opacity);
    }

    if let Some(opacity) = shared_opacity {
        write_opacity(out, "opacity", opacity);
    }

    out.push_str("/>");
}

fn write_stroke(out: &mut String, stroke: &StrokeItem, write_stroke_opacity: bool) {
    write_hex(out, "stroke", stroke.rgb);
    let _ = write!(out, " stroke-width=\"{}\"", number(stroke.width));
    match stroke.linecap {
        LineCap::Butt => {}
        LineCap::Round => out.push_str(" stroke-linecap=\"round\""),
        LineCap::Square => out.push_str(" stroke-linecap=\"square\""),
    }
    match stroke.linejoin {
        LineJoin::Miter => {
            if (stroke.miterlimit - 4.0).abs() > 0.001 {
                let _ = write!(out, " stroke-miterlimit=\"{}\"", number(stroke.miterlimit));
            }
        }
        LineJoin::MiterClip => out.push_str(" stroke-linejoin=\"miter\""),
        LineJoin::Round => out.push_str(" stroke-linejoin=\"round\""),
        LineJoin::Bevel => out.push_str(" stroke-linejoin=\"bevel\""),
    }
    if !stroke.dasharray.is_empty() {
        out.push_str(" stroke-dasharray=\"");
        for (idx, dash) in stroke.dasharray.iter().enumerate() {
            if idx > 0 {
                out.push(' ');
            }
            out.push_str(&number(*dash));
        }
        out.push('"');
    }
    if stroke.dashoffset.abs() > 0.001 {
        let _ = write!(out, " stroke-dashoffset=\"{}\"", number(stroke.dashoffset));
    }
    if write_stroke_opacity {
        write_opacity(out, "stroke-opacity", stroke.opacity);
    }
}

fn resolve_opacity(fill: Option<&FillItem>, stroke: Option<&StrokeItem>) -> (Option<f32>, bool) {
    match (fill, stroke) {
        (Some(f), None) => (Some(f.opacity), false),
        (None, Some(s)) => (Some(s.opacity), false),
        (Some(f), Some(s)) if (f.opacity - s.opacity).abs() < 0.0005 => (Some(f.opacity), false),
        (Some(_), Some(_)) => (None, true),
        (None, None) => (None, false),
    }
}

fn write_hex(out: &mut String, attr: &str, [r, g, b]: [u8; 3]) {
    let _ = write!(out, " {attr}=\"#{r:02x}{g:02x}{b:02x}\"");
}

fn write_opacity(out: &mut String, attr: &str, value: f32) {
    let clamped = value.clamp(0.0, 1.0);
    if clamped < 1.0 {
        let formatted = number(clamped);
        if formatted != "1" {
            let _ = write!(out, " {attr}=\"{formatted}\"");
        }
    }
}
