use std::io::Cursor;
use std::sync::{Arc, LazyLock};

use gpui_kit::RenderImage;
use image::metadata::Orientation;
use image::{DynamicImage, Frame, ImageError, ImageFormat, ImageReader, RgbaImage};
use resvg::{tiny_skia, usvg};

use super::exif::{self, ExifEntry};

pub const IMAGE_LIMIT: u64 = 256 * 1024 * 1024;
const MAX_SIDE: u32 = 8192;
const MAX_PIXELS: f64 = 24_000_000.;
const SVG_SIDE: f32 = 2048.;

pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub format: &'static str,
    pub bytes: u64,
    pub vector: bool,
    pub exif: Vec<ExifEntry>,
    pub pixels: Arc<RenderImage>,
}

fn extension(name: &str) -> Option<String> {
    let (_, extension) = name.rsplit_once('.')?;
    Some(extension.to_ascii_lowercase())
}

fn is_svg_name(name: &str) -> bool {
    extension(name).is_some_and(|extension| extension == "svg" || extension == "svgz")
}

fn has_svg_tag(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(4096)];
    head.windows(4).any(|window| window == b"<svg")
}

fn format_name(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "PNG",
        ImageFormat::Jpeg => "JPEG",
        ImageFormat::Gif => "GIF",
        ImageFormat::WebP => "WebP",
        ImageFormat::Bmp => "BMP",
        ImageFormat::Ico => "ICO",
        ImageFormat::Tiff => "TIFF",
        other => other.extensions_str().first().copied().unwrap_or("Image"),
    }
}

fn describe(error: &ImageError) -> String {
    match error {
        ImageError::Limits(_) => "This image is too large to show here.".to_owned(),
        ImageError::Unsupported(_) => "This image type is not supported.".to_owned(),
        other => format!("Cannot show this image: {other}"),
    }
}

fn display_size(width: f32, height: f32) -> (u32, u32) {
    let side = (MAX_SIDE as f32 / width.max(height)).min(1.);
    let area = (MAX_PIXELS / (f64::from(width) * f64::from(height))).sqrt() as f32;
    let shrink = side.min(area.min(1.));
    (
        ((width * shrink).round() as u32).max(1),
        ((height * shrink).round() as u32).max(1),
    )
}

fn to_bgra(mut rgba: RgbaImage) -> Arc<RenderImage> {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Arc::new(RenderImage::new(vec![Frame::new(rgba)]))
}

fn shrunk(image: &DynamicImage) -> RgbaImage {
    let (width, height) = display_size(image.width() as f32, image.height() as f32);
    if (width, height) == (image.width(), image.height()) {
        image.to_rgba8()
    } else {
        image.thumbnail_exact(width, height).into_rgba8()
    }
}

fn raster(bytes: &[u8], format: ImageFormat) -> Result<Picture, String> {
    let mut reader = ImageReader::new(Cursor::new(bytes));
    reader.set_format(format);
    let mut image = reader.decode().map_err(|error| describe(&error))?;
    let info = exif::read(bytes);
    if let Some(turn) = info.orientation.and_then(Orientation::from_exif) {
        image.apply_orientation(turn);
    }
    Ok(Picture {
        width: image.width(),
        height: image.height(),
        format: format_name(format),
        bytes: bytes.len() as u64,
        vector: false,
        exif: info.entries,
        pixels: to_bgra(shrunk(&image)),
    })
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

fn vector(bytes: &[u8]) -> Result<Picture, String> {
    let tree = usvg::Tree::from_data(bytes, &svg_options())
        .map_err(|error| format!("Cannot show this SVG: {error}"))?;
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
        bytes: bytes.len() as u64,
        vector: true,
        exif: Vec::new(),
        pixels,
    })
}

fn guess(bytes: &[u8], name: &str) -> Option<ImageFormat> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    reader
        .format()
        .or_else(|| ImageFormat::from_extension(extension(name)?))
}

pub fn decode_image(bytes: &[u8], name: &str) -> Result<Picture, String> {
    if is_svg_name(name) {
        return vector(bytes);
    }
    match guess(bytes, name) {
        Some(format) => raster(bytes, format),
        None if has_svg_tag(bytes) => vector(bytes),
        None => Err("This file is not an image Ferrite can show.".to_owned()),
    }
}
