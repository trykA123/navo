//! The SVG normaliser (BB-D9): refuses hostile input before parsing, parses with no resolver,
//! keeps only solid paths and writes them back with our own serializer.

use std::collections::BTreeSet;

use resvg::usvg::{
    Fill, FillRule, Group, ImageHrefResolver, Node, Options, Paint, PaintOrder, Path, Tree,
    roxmltree,
};

use write::{Line, Shape};

mod weigh;
mod write;

pub use weigh::{MAX_DEPTH, MAX_WEIGHT};

/// The largest SVG file accepted; the caller reads with this cap.
pub const SVG_CAP: usize = 512 * 1024;
/// The most paths one icon may have.
pub const MAX_PATHS: usize = 10_000;

/// Something the normaliser dropped, for the import notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Removed {
    /// `<text>` elements.
    Text,
    /// `<image>` elements.
    Image,
    /// Filters on groups.
    Filter,
    /// Masks on groups.
    Mask,
    /// Clip paths on groups.
    ClipPath,
    /// Gradient paints, turned into their first stop colour.
    Gradient,
    /// Pattern paints, turned into no paint.
    Pattern,
}

impl Removed {
    /// The words the import notice lists.
    pub fn label(self) -> &'static str {
        match self {
            Removed::Text => "text",
            Removed::Image => "embedded images",
            Removed::Filter => "filters",
            Removed::Mask => "masks",
            Removed::ClipPath => "clip paths",
            Removed::Gradient => "gradients",
            Removed::Pattern => "patterns",
        }
    }
}

/// A cleaned SVG and what was dropped to get it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cleaned {
    /// The normalised document.
    pub svg: Vec<u8>,
    /// What was dropped, sorted and unique.
    pub removed: Vec<Removed>,
}

/// Why an SVG is refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SvgError {
    /// Over `SVG_CAP`, as read or as cleaned.
    #[error("the SVG is larger than 512 KiB")]
    TooLarge,
    /// A gzip-compressed `.svgz`.
    #[error("compressed SVG files are not supported")]
    Compressed,
    /// Not UTF-8 text.
    #[error("the SVG is not UTF-8 text")]
    NotUtf8,
    /// Has a `<!DOCTYPE` or `<!ENTITY`.
    #[error("the SVG declares a document type or entities")]
    Doctype,
    /// The parser refused it.
    #[error("the SVG cannot be read: {0}")]
    Parse(String),
    /// More than `MAX_PATHS` paths.
    #[error("the SVG has more than 10,000 paths")]
    TooManyPaths,
    /// No path is left, or the canvas rounds to zero.
    #[error("the SVG has nothing to draw")]
    Empty,
    /// Expands past `MAX_WEIGHT` through `<use>`, markers, masks, clip paths or filters.
    #[error("the SVG repeats its shapes too many times")]
    TooComplex,
    /// Nests elements or references deeper than `MAX_DEPTH`.
    #[error("the SVG nests too deeply")]
    TooDeep,
}

/// Cleans `bytes` down to solid paths; with `mono` every colour is black.
///
/// Steps of BB-D9 in order: refuse before parsing (size, entities, expansion, nesting), parse
/// with no resolver, walk the tree, write our own document, refuse too many paths, none or an
/// output over `SVG_CAP`.
pub fn normalise(bytes: &[u8], mono: bool) -> Result<Cleaned, SvgError> {
    let text = screen(bytes)?;
    let document =
        roxmltree::Document::parse(text).map_err(|error| SvgError::Parse(error.to_string()))?;
    weigh::check(&document)?;
    let mut removed = dropped_elements(&document);
    let tree =
        Tree::from_str(text, &options()).map_err(|error| SvgError::Parse(error.to_string()))?;
    let shapes = shapes(tree.root(), &mut removed)?;
    let size = tree.size();
    if shapes.is_empty()
        || write::number(size.width()) == "0"
        || write::number(size.height()) == "0"
    {
        return Err(SvgError::Empty);
    }
    let svg = write::document(size.width(), size.height(), &shapes, mono);
    if svg.len() > SVG_CAP {
        return Err(SvgError::TooLarge);
    }
    Ok(Cleaned {
        svg,
        removed: removed.into_iter().collect(),
    })
}

/// Step 1: the checks that run before any parser sees the bytes.
fn screen(bytes: &[u8]) -> Result<&str, SvgError> {
    if bytes.len() > SVG_CAP {
        return Err(SvgError::TooLarge);
    }
    if bytes.starts_with(&[0x1F, 0x8B]) {
        return Err(SvgError::Compressed);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| SvgError::NotUtf8)?;
    let declares = |word: &[u8]| {
        bytes
            .windows(word.len())
            .any(|window| window.eq_ignore_ascii_case(word))
    };
    if declares(b"<!doctype") || declares(b"<!entity") {
        return Err(SvgError::Doctype);
    }
    Ok(text)
}

/// Parser options that never read a file, a font or a URL: both image resolvers answer `None`,
/// there is no resources folder and the font database is empty.
fn options() -> Options<'static> {
    Options {
        resources_dir: None,
        image_href_resolver: ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Options::default()
    }
}

/// `<text>` and `<image>` in the source: the parser drops them silently (no fonts, no resolver),
/// so the notice reads them from the XML.
fn dropped_elements(document: &roxmltree::Document<'_>) -> BTreeSet<Removed> {
    let mut removed = BTreeSet::new();
    for node in document
        .descendants()
        .filter(|node| node.tag_name().namespace() == Some(SVG_NS))
    {
        match node.tag_name().name() {
            "text" => removed.insert(Removed::Text),
            "image" => removed.insert(Removed::Image),
            _ => false,
        };
    }
    removed
}

const SVG_NS: &str = "http://www.w3.org/2000/svg";

/// Step 3: every visible path in document order with solid paints; groups keep their children
/// and lose filters, masks and clip paths.
fn shapes(root: &Group, removed: &mut BTreeSet<Removed>) -> Result<Vec<Shape>, SvgError> {
    let mut found = Vec::new();
    let mut paths = 0_usize;
    let mut stack: Vec<(&Node, f32)> = root
        .children()
        .iter()
        .rev()
        .map(|node| (node, root.opacity().get()))
        .collect();
    while let Some((node, opacity)) = stack.pop() {
        match node {
            Node::Group(group) => {
                note_group(group, removed);
                let opacity = opacity * group.opacity().get();
                stack.extend(group.children().iter().rev().map(|child| (child, opacity)));
            }
            Node::Path(path) => {
                paths += 1;
                if paths > MAX_PATHS {
                    return Err(SvgError::TooManyPaths);
                }
                found.extend(shape(path, opacity, removed));
            }
            Node::Image(_) => {
                removed.insert(Removed::Image);
            }
            Node::Text(_) => {
                removed.insert(Removed::Text);
            }
        }
    }
    Ok(found)
}

fn note_group(group: &Group, removed: &mut BTreeSet<Removed>) {
    if !group.filters().is_empty() {
        removed.insert(Removed::Filter);
    }
    if group.mask().is_some() {
        removed.insert(Removed::Mask);
    }
    if group.clip_path().is_some() {
        removed.insert(Removed::ClipPath);
    }
}

/// One path as zero, one or two shapes: one when fill and stroke share an opacity and the
/// default paint order, else one per paint in paint order.
fn shape(path: &Path, opacity: f32, removed: &mut BTreeSet<Removed>) -> Vec<Shape> {
    if !path.is_visible() {
        return Vec::new();
    }
    let transform = path.abs_transform();
    let Some(data) = path.data().clone().transform(transform) else {
        return Vec::new();
    };
    let fill = path.fill().and_then(|fill| {
        let (rgb, alpha) = solid(fill.paint(), removed)?;
        Some((rgb, alpha * fill.opacity().get()))
    });
    let stroke = path.stroke().and_then(|stroke| {
        let (rgb, alpha) = solid(stroke.paint(), removed)?;
        let (scale_x, scale_y) = transform.get_scale();
        let width = stroke.width().get() * (scale_x * scale_y).sqrt();
        let line = Line {
            rgb,
            width,
            cap: stroke.linecap(),
            join: stroke.linejoin(),
        };
        Some((line, alpha * stroke.opacity().get()))
    });
    let rule = path.fill().map_or(FillRule::NonZero, Fill::rule);
    let base = Shape {
        data,
        fill: None,
        rule,
        stroke: None,
        opacity,
    };
    match (fill, stroke) {
        (None, None) => Vec::new(),
        (Some((rgb, alpha)), None) => vec![Shape {
            fill: Some(rgb),
            opacity: opacity * alpha,
            ..base
        }],
        (None, Some((line, alpha))) => vec![Shape {
            stroke: Some(line),
            opacity: opacity * alpha,
            ..base
        }],
        (Some((rgb, fill_alpha)), Some((line, line_alpha))) => {
            if fill_alpha == line_alpha && path.paint_order() == PaintOrder::FillAndStroke {
                return vec![Shape {
                    fill: Some(rgb),
                    stroke: Some(line),
                    opacity: opacity * fill_alpha,
                    ..base
                }];
            }
            let filled = Shape {
                fill: Some(rgb),
                opacity: opacity * fill_alpha,
                ..base.clone()
            };
            let stroked = Shape {
                stroke: Some(line),
                opacity: opacity * line_alpha,
                ..base
            };
            match path.paint_order() {
                PaintOrder::FillAndStroke => vec![filled, stroked],
                PaintOrder::StrokeAndFill => vec![stroked, filled],
            }
        }
    }
}

/// A paint as one colour and its alpha: a gradient gives its first stop, a pattern nothing.
fn solid(paint: &Paint, removed: &mut BTreeSet<Removed>) -> Option<([u8; 3], f32)> {
    let gradient = match paint {
        Paint::Color(colour) => return Some(([colour.red, colour.green, colour.blue], 1.0)),
        Paint::LinearGradient(gradient) => gradient.stops(),
        Paint::RadialGradient(gradient) => gradient.stops(),
        Paint::Pattern(_) => {
            removed.insert(Removed::Pattern);
            return None;
        }
    };
    removed.insert(Removed::Gradient);
    let stop = gradient.first()?;
    let colour = stop.color();
    Some((
        [colour.red, colour.green, colour.blue],
        stop.opacity().get(),
    ))
}

#[cfg(test)]
mod tests;
