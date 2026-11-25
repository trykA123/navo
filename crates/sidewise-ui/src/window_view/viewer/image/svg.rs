use std::borrow::Cow;
use std::io::Read;
use std::sync::{Arc, LazyLock, OnceLock};

use flate2::read::GzDecoder;
use image::RgbaImage;
use resvg::{tiny_skia, usvg};

use super::decode::{IMAGE_LIMIT, Picture, display_size};

const SVG_SIDE: f32 = 2048.;
const GZIP: [u8; 2] = [0x1f, 0x8b];
const SVG_STACK: usize = 16 << 20;
const MAX_DEPTH: usize = 256;
const MAX_LAYERS: usize = 16;
const NESTED: &str = "This SVG is nested too deeply to show here.";

#[doc(hidden)]
pub fn unzipped(bytes: &[u8], limit: u64) -> Result<Cow<'_, [u8]>, String> {
    if !bytes.starts_with(&GZIP) {
        return Ok(Cow::Borrowed(bytes));
    }
    let mut plain = Vec::new();
    GzDecoder::new(bytes)
        .take(limit.saturating_add(1))
        .read_to_end(&mut plain)
        .map_err(|error| format!("Cannot show this SVG: {error}"))?;
    if plain.len() as u64 > limit {
        return Err(format!(
            "This SVG is too large to show here (over {} MB unpacked).",
            limit / (1024 * 1024)
        ));
    }
    if plain.starts_with(&GZIP) {
        return Err("Cannot show this SVG: it is packed twice.".to_owned());
    }
    Ok(Cow::Owned(plain))
}

fn after<'a>(rest: &'a [u8], close: &[u8]) -> &'a [u8] {
    let end = rest.windows(close.len()).position(|window| window == close);
    end.map_or(&[], |end| &rest[end + close.len()..])
}

fn too_deep(text: &[u8]) -> bool {
    let mut depth = 0usize;
    let mut rest = text;
    while let Some(at) = rest.iter().position(|byte| *byte == b'<') {
        rest = &rest[at + 1..];
        if rest.starts_with(b"!--") {
            rest = after(rest, b"-->");
            continue;
        }
        if rest.starts_with(b"![CDATA[") {
            rest = after(rest, b"]]>");
            continue;
        }
        let end = rest
            .iter()
            .position(|byte| *byte == b'>')
            .unwrap_or(rest.len());
        let tag = &rest[..end];
        match tag.first() {
            Some(b'/') => depth = depth.saturating_sub(1),
            Some(b'!' | b'?') => {}
            _ if tag.ends_with(b"/") => {}
            _ => depth += 1,
        }
        if depth > MAX_DEPTH {
            return true;
        }
        rest = &rest[end..];
    }
    false
}

fn too_many_layers(root: &usvg::Group) -> bool {
    let mut groups = vec![(root, 0usize)];
    while let Some((group, outer)) = groups.pop() {
        let layers = outer + usize::from(group.should_isolate());
        if layers > MAX_LAYERS {
            return true;
        }
        groups.extend(group.children().iter().filter_map(|node| match node {
            usvg::Node::Group(child) => Some((child.as_ref(), layers)),
            _ => None,
        }));
    }
    false
}

/// System fonts, loaded once and only when an SVG has text (loading takes ~0.5 s+).
static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
static NO_FONTS: LazyLock<Arc<usvg::fontdb::Database>> =
    LazyLock::new(|| Arc::new(usvg::fontdb::Database::new()));
const TEXT_TAGS: [&[u8]; 4] = [b"<text", b":text", b"<tspan", b":tspan"];

/// Whether the SVG may draw text; false positives only cost the font load.
fn has_text(bytes: &[u8]) -> bool {
    TEXT_TAGS
        .iter()
        .any(|tag| bytes.windows(tag.len()).any(|window| window == *tag))
}

fn system_fonts() -> Arc<usvg::fontdb::Database> {
    FONTS
        .get_or_init(|| {
            let mut fonts = usvg::fontdb::Database::new();
            fonts.load_system_fonts();
            Arc::new(fonts)
        })
        .clone()
}

fn svg_options(bytes: &[u8]) -> usvg::Options<'static> {
    let fontdb = if has_text(bytes) {
        system_fonts()
    } else {
        NO_FONTS.clone()
    };
    usvg::Options {
        fontdb,
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    }
}

fn unpremultiply(pixel: &mut [u8; 4]) {
    pixel.swap(0, 2);
    let alpha = u16::from(pixel[3]);
    if alpha == 0 || alpha == 255 {
        return;
    }
    for channel in &mut pixel[..3] {
        *channel = (u16::from(*channel) * 255 / alpha).min(255) as u8;
    }
}

pub(super) fn vector(bytes: &[u8]) -> Result<Picture, String> {
    let plain = unzipped(bytes, IMAGE_LIMIT)?;
    if too_deep(&plain) {
        return Err(NESTED.to_owned());
    }
    let size = bytes.len() as u64;
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("svg".to_owned())
            .stack_size(SVG_STACK)
            .spawn_scoped(scope, || rendered(&plain, size))
            .map_err(|error| format!("Cannot show this SVG: {error}"))?
            .join()
            .map_err(|_| "Cannot show this SVG: drawing it failed.".to_owned())?
    })
}

fn rendered(bytes: &[u8], size: u64) -> Result<Picture, String> {
    let tree = usvg::Tree::from_data(bytes, &svg_options(bytes))
        .map_err(|error| format!("Cannot show this SVG: {error}"))?;
    if too_many_layers(tree.root()) {
        return Err(NESTED.to_owned());
    }
    let (width, height) = (tree.size().width(), tree.size().height());
    let wanted = SVG_SIDE.max(width.max(height));
    let (pixels_wide, pixels_tall) = display_size(
        width * wanted / width.max(height),
        height * wanted / width.max(height),
    );
    let mut pixmap = tiny_skia::Pixmap::new(pixels_wide, pixels_tall)
        .ok_or_else(|| "Cannot show this SVG: it has no size.".to_owned())?;
    let transform =
        tiny_skia::Transform::from_scale(pixels_wide as f32 / width, pixels_tall as f32 / height);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let mut data = pixmap.take();
    data.as_chunks_mut::<4>()
        .0
        .iter_mut()
        .for_each(unpremultiply);
    let buffer = RgbaImage::from_raw(pixels_wide, pixels_tall, data)
        .ok_or_else(|| "Cannot show this SVG.".to_owned())?;
    Ok(Picture {
        width: width.round().max(1.) as u32,
        height: height.round().max(1.) as u32,
        format: "SVG",
        bytes: size,
        vector: true,
        exif: Vec::new(),
        pixels: buffer,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHAPES: &[u8] =
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><style type="text/css"/><rect width="8" height="8"/></svg>"#;
    const WORDS: &[u8] =
        br#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><text y="8">Hi<tspan>!</tspan></text></svg>"#;

    #[test]
    fn a_text_free_svg_never_loads_system_fonts() {
        assert!(svg_options(SHAPES).fontdb.is_empty());
        vector(SHAPES).unwrap();
        assert!(FONTS.get().is_none(), "system fonts loaded");
    }

    #[test]
    fn an_svg_with_text_loads_system_fonts() {
        vector(WORDS).unwrap();
        assert!(FONTS.get().is_some());
    }
}
