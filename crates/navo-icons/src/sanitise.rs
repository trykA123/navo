//! Security-hardened SVG normaliser with strict allowlist XML pre-parsing.

mod preparse;
mod write;

use std::collections::BTreeSet;
use std::sync::Arc;

use resvg::usvg::{Group, ImageHrefResolver, Node, Options, Paint, Path, Transform, Tree, fontdb};

use self::write::{FillItem, PathItem, StrokeItem, path_data, write_svg};

/// Maximum raw byte length accepted for an SVG icon import (512 KiB).
pub const SVG_CAP: usize = 512 * 1024;
/// Maximum number of paths emitted in a normalised SVG.
pub const MAX_PATHS: usize = 10_000;

/// Feature stripped from an SVG during normalisation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Removed {
    /// Text nodes (`text`, `tspan`, `textPath`).
    Text,
    /// Embedded raster or external `<image>` nodes.
    Image,
    /// SVG filter definitions or attributes.
    Filter,
    /// SVG mask definitions or attributes.
    Mask,
    /// SVG clip-path definitions or attributes.
    ClipPath,
    /// Gradient paints converted to their first stop colour.
    Gradient,
    /// Pattern definitions or paints.
    Pattern,
}

impl Removed {
    /// Human-readable label for a stripped SVG feature.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Image => "embedded images",
            Self::Filter => "filters",
            Self::Mask => "masks",
            Self::ClipPath => "clip paths",
            Self::Gradient => "gradients",
            Self::Pattern => "patterns",
        }
    }
}

/// Normalised SVG bytes and the sorted, deduplicated list of stripped features.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cleaned {
    /// Normalised UTF-8 `<svg>...</svg>` bytes containing only `<path>` elements.
    pub svg: Vec<u8>,
    /// Sorted, unique features removed or simplified during normalisation.
    pub removed: Vec<Removed>,
}

/// Why an untrusted SVG could not be normalised.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SvgError {
    /// Raw input exceeded [`SVG_CAP`].
    #[error("SVG exceeds maximum size of 512 KiB")]
    TooLarge,
    /// Input starts with the gzip magic header (`.svgz`).
    #[error("compressed SVG (.svgz) is not supported")]
    Compressed,
    /// Input bytes are not valid UTF-8.
    #[error("SVG is not valid UTF-8")]
    NotUtf8,
    /// Input contains a `<!DOCTYPE` or `<!ENTITY` declaration.
    #[error("SVG contains a disallowed DOCTYPE or ENTITY declaration")]
    Doctype,
    /// XML or SVG parsing failed or violated structural limits.
    #[error("failed to parse SVG: {0}")]
    Parse(String),
    /// Document exceeded [`MAX_PATHS`] or XML element limits.
    #[error("SVG exceeds maximum path limit")]
    TooManyPaths,
    /// Normalisation produced no visible paths.
    #[error("SVG contains no drawable paths")]
    Empty,
}

/// Normalises an untrusted SVG into a minimal, safe `<svg>` containing only `<path>` elements.
pub fn normalise(bytes: &[u8], mono: bool) -> Result<Cleaned, SvgError> {
    normalise_with_resolver(bytes, mono, deny_all_image_resolver())
}

fn normalise_with_resolver(
    bytes: &[u8],
    mono: bool,
    resolver: ImageHrefResolver<'_>,
) -> Result<Cleaned, SvgError> {
    let mut preparsed = preparse::preparse(bytes)?;
    let options = Options {
        resources_dir: None,
        fontdb: Arc::new(fontdb::Database::new()),
        image_href_resolver: resolver,
        ..Options::default()
    };
    let tree =
        Tree::from_str(&preparsed.xml, &options).map_err(|err| SvgError::Parse(err.to_string()))?;

    let size = tree.size();
    let (width, height) = (size.width(), size.height());
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(SvgError::Parse(
            "SVG has non-positive dimensions".to_owned(),
        ));
    }

    let mut paths = Vec::new();
    walk_group(tree.root(), 1.0, mono, &mut preparsed.removed, &mut paths)?;

    if paths.is_empty() {
        return Err(SvgError::Empty);
    }

    Ok(Cleaned {
        svg: write_svg(width, height, &paths),
        removed: preparsed.removed.into_iter().collect(),
    })
}

fn deny_all_image_resolver() -> ImageHrefResolver<'static> {
    ImageHrefResolver {
        resolve_data: Box::new(|_, _, _| None),
        resolve_string: Box::new(|_, _| None),
    }
}

fn walk_group(
    group: &Group,
    parent_opacity: f32,
    mono: bool,
    removed: &mut BTreeSet<Removed>,
    out: &mut Vec<PathItem>,
) -> Result<(), SvgError> {
    if !group.filters().is_empty() {
        removed.insert(Removed::Filter);
    }
    if group.mask().is_some() {
        removed.insert(Removed::Mask);
    }
    if group.clip_path().is_some() {
        removed.insert(Removed::ClipPath);
    }

    let opacity = (parent_opacity * group.opacity().get()).clamp(0.0, 1.0);
    for child in group.children() {
        match child {
            Node::Group(sub) => walk_group(sub, opacity, mono, removed, out)?,
            Node::Path(path) => collect_path(path, opacity, mono, removed, out)?,
            Node::Image(_) => {
                removed.insert(Removed::Image);
            }
            Node::Text(_) => {
                removed.insert(Removed::Text);
            }
        }
    }
    Ok(())
}

fn collect_path(
    path: &Path,
    group_opacity: f32,
    mono: bool,
    removed: &mut BTreeSet<Removed>,
    out: &mut Vec<PathItem>,
) -> Result<(), SvgError> {
    if !path.is_visible() {
        return Ok(());
    }

    let fill = path.fill().and_then(|f| {
        let (rgb, stop_op) = resolve_paint(f.paint(), mono, removed)?;
        let opacity = (group_opacity * f.opacity().get() * stop_op).clamp(0.0, 1.0);
        Some(FillItem {
            rgb,
            opacity,
            rule: f.rule(),
        })
    });

    let scale = transform_scale(path.abs_transform());
    let stroke = path.stroke().and_then(|s| {
        let (rgb, stop_op) = resolve_paint(s.paint(), mono, removed)?;
        let opacity = (group_opacity * s.opacity().get() * stop_op).clamp(0.0, 1.0);
        let dasharray = s
            .dasharray()
            .map(|list| list.iter().map(|v| *v * scale).collect())
            .unwrap_or_default();
        Some(StrokeItem {
            rgb,
            opacity,
            width: s.width().get() * scale,
            linecap: s.linecap(),
            linejoin: s.linejoin(),
            miterlimit: s.miterlimit().get(),
            dasharray,
            dashoffset: s.dashoffset() * scale,
        })
    });

    if fill.is_none() && stroke.is_none() {
        return Ok(());
    }

    if out.len() >= MAX_PATHS {
        return Err(SvgError::TooManyPaths);
    }

    let Some(transformed) = path.data().clone().transform(path.abs_transform()) else {
        return Ok(());
    };
    let d = path_data(&transformed);
    if d.is_empty() {
        return Ok(());
    }

    out.push(PathItem { d, fill, stroke });
    Ok(())
}

fn resolve_paint(
    paint: &Paint,
    mono: bool,
    removed: &mut BTreeSet<Removed>,
) -> Option<([u8; 3], f32)> {
    let (rgb, opacity) = match paint {
        Paint::Color(c) => ([c.red, c.green, c.blue], 1.0),
        Paint::LinearGradient(grad) => {
            removed.insert(Removed::Gradient);
            let stop = grad.stops().first()?;
            let c = stop.color();
            ([c.red, c.green, c.blue], stop.opacity().get())
        }
        Paint::RadialGradient(grad) => {
            removed.insert(Removed::Gradient);
            let stop = grad.stops().first()?;
            let c = stop.color();
            ([c.red, c.green, c.blue], stop.opacity().get())
        }
        Paint::Pattern(_) => {
            removed.insert(Removed::Pattern);
            return None;
        }
    };
    let final_rgb = if mono { [0, 0, 0] } else { rgb };
    Some((final_rgb, opacity))
}

fn transform_scale(ts: Transform) -> f32 {
    let sx = ts.sx.hypot(ts.ky);
    let sy = ts.kx.hypot(ts.sy);
    let scale = (sx * sy).sqrt();
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests;
