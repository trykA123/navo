use std::borrow::Cow;
use std::io::Read;
use std::sync::{Arc, LazyLock};

use flate2::read::GzDecoder;
use gpui_kit::RenderImage;
use image::{Frame, RgbaImage};
use resvg::{tiny_skia, usvg};

use super::decode::{IMAGE_LIMIT, Picture, display_size};

const SVG_SIDE: f32 = 2048.;
const GZIP: [u8; 2] = [0x1f, 0x8b];
const SVG_STACK: usize = 16 << 20;
const MAX_DEPTH: usize = 256;
const MAX_LAYERS: usize = 16;
const NESTED: &str = "This SVG is nested too deeply to show here.";

pub(super) fn unzipped(bytes: &[u8], limit: u64) -> Result<Cow<'_, [u8]>, String> {
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

static FONTS: LazyLock<Arc<usvg::fontdb::Database>> = LazyLock::new(|| {
    let mut fonts = usvg::fontdb::Database::new();
    fonts.load_system_fonts();
    Arc::new(fonts)
});

fn svg_options() -> usvg::Options<'static> {
    usvg::Options {
        fontdb: FONTS.clone(),
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
    let tree = usvg::Tree::from_data(bytes, &svg_options())
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
    let pixels = Arc::new(RenderImage::new(vec![Frame::new(buffer)]));
    Ok(Picture {
        width: width.round().max(1.) as u32,
        height: height.round().max(1.) as u32,
        format: "SVG",
        bytes: size,
        vector: true,
        exif: Vec::new(),
        pixels,
    })
}
