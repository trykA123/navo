//! Our own SVG serializer: it can only write `<svg viewBox>` and `<path>` with a fixed set of
//! attributes holding numbers, path commands and `#rrggbb` colours, so nothing else survives.

use std::fmt::Write as _;

use resvg::usvg::tiny_skia_path::{Path, PathSegment};
use resvg::usvg::{FillRule, LineCap, LineJoin};

/// A solid stroke as it is written.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Line {
    pub(super) rgb: [u8; 3],
    pub(super) width: f32,
    pub(super) cap: LineCap,
    pub(super) join: LineJoin,
}

/// One `<path>` element: data already in document space, solid paints, one opacity.
#[derive(Clone, Debug)]
pub(super) struct Shape {
    pub(super) data: Path,
    pub(super) fill: Option<[u8; 3]>,
    pub(super) rule: FillRule,
    pub(super) stroke: Option<Line>,
    pub(super) opacity: f32,
}

/// The whole document; with `mono` every colour is `#000000`.
pub(super) fn document(width: f32, height: f32, shapes: &[Shape], mono: bool) -> Vec<u8> {
    let mut out = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}">"#,
        number(width),
        number(height)
    );
    for shape in shapes {
        element(&mut out, shape, mono);
    }
    out.push_str("</svg>");
    out.into_bytes()
}

/// Appends one `<path …/>`.
fn element(out: &mut String, shape: &Shape, mono: bool) {
    let _ = write!(out, r#"<path d="{}""#, path_data(&shape.data));
    match shape.fill {
        Some(rgb) => {
            let _ = write!(out, r#" fill="{}""#, colour(rgb, mono));
            if shape.rule == FillRule::EvenOdd {
                out.push_str(r#" fill-rule="evenodd""#);
            }
        }
        None => out.push_str(r#" fill="none""#),
    }
    if let Some(line) = shape.stroke {
        let _ = write!(
            out,
            r#" stroke="{}" stroke-width="{}""#,
            colour(line.rgb, mono),
            number(line.width)
        );
        if let Some(cap) = cap_name(line.cap) {
            let _ = write!(out, r#" stroke-linecap="{cap}""#);
        }
        if let Some(join) = join_name(line.join) {
            let _ = write!(out, r#" stroke-linejoin="{join}""#);
        }
    }
    let opacity = number(shape.opacity);
    if opacity != "1" {
        let _ = write!(out, r#" opacity="{opacity}""#);
    }
    out.push_str("/>");
}

/// The non-default cap name.
fn cap_name(cap: LineCap) -> Option<&'static str> {
    match cap {
        LineCap::Butt => None,
        LineCap::Round => Some("round"),
        LineCap::Square => Some("square"),
    }
}

/// The non-default join name.
fn join_name(join: LineJoin) -> Option<&'static str> {
    match join {
        LineJoin::Miter => None,
        LineJoin::MiterClip => Some("miter-clip"),
        LineJoin::Round => Some("round"),
        LineJoin::Bevel => Some("bevel"),
    }
}

/// `#rrggbb`, or black for a mono icon (GPUI tints the mask).
fn colour([red, green, blue]: [u8; 3], mono: bool) -> String {
    if mono {
        return "#000000".to_owned();
    }
    format!("#{red:02x}{green:02x}{blue:02x}")
}

/// At most 3 decimals, trailing zeros and `-0` trimmed; a non-finite value is `0`.
pub(super) fn number(value: f32) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    match text {
        "-0" | "" => "0".to_owned(),
        text => text.to_owned(),
    }
}

/// Absolute `M`, `L`, `Q`, `C` and `Z` commands.
pub(super) fn path_data(path: &Path) -> String {
    let mut out = String::new();
    for segment in path.segments() {
        let (command, points) = match segment {
            PathSegment::MoveTo(to) => ('M', vec![to]),
            PathSegment::LineTo(to) => ('L', vec![to]),
            PathSegment::QuadTo(control, to) => ('Q', vec![control, to]),
            PathSegment::CubicTo(first, second, to) => ('C', vec![first, second, to]),
            PathSegment::Close => ('Z', Vec::new()),
        };
        out.push(command);
        let coordinates: Vec<String> = points
            .iter()
            .flat_map(|point| [number(point.x), number(point.y)])
            .collect();
        out.push_str(&coordinates.join(" "));
    }
    out
}
