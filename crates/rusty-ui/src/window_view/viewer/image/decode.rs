use std::io::Cursor;
use std::sync::Arc;

use gpui_kit::RenderImage;
use image::metadata::Orientation;
use image::{
    DynamicImage, Frame, GenericImageView, ImageDecoder, ImageError, ImageFormat, ImageReader,
    Limits, RgbaImage,
};

use super::exif::{self, ExifEntry};
use super::svg::vector;

pub const IMAGE_LIMIT: u64 = 256 * 1024 * 1024;
const DECODE_BUDGET: u64 = 256 * 1024 * 1024;
const TIFF_EXIF_SPAN: usize = 4 * 1024 * 1024;
const MAX_SIDE: u32 = 8192;
const MAX_PIXELS: f64 = 24_000_000.;

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

pub(super) fn display_size(width: f32, height: f32) -> (u32, u32) {
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

fn shrunk(image: DynamicImage) -> RgbaImage {
    let (width, height) = display_size(image.width() as f32, image.height() as f32);
    if (width, height) == (image.width(), image.height()) {
        image.into_rgba8()
    } else {
        image.thumbnail_exact(width, height).into_rgba8()
    }
}

fn turned_size((width, height): (u32, u32), turn: Orientation) -> (u32, u32) {
    match turn {
        Orientation::Rotate90
        | Orientation::Rotate270
        | Orientation::Rotate90FlipH
        | Orientation::Rotate270FlipH => (height, width),
        _ => (width, height),
    }
}

fn raster(bytes: &[u8], format: ImageFormat) -> Result<Picture, String> {
    let mut reader = ImageReader::new(Cursor::new(bytes));
    reader.set_format(format);
    let mut limits = Limits::default();
    limits.max_alloc = Some(DECODE_BUDGET);
    reader.limits(limits.clone());
    let mut decoder = reader.into_decoder().map_err(|error| describe(&error))?;
    limits
        .reserve(decoder.total_bytes())
        .map_err(|error| describe(&error))?;
    let raw = match format {
        ImageFormat::Tiff => Some(bytes[..bytes.len().min(TIFF_EXIF_SPAN)].to_vec()),
        _ => decoder.exif_metadata().ok().flatten(),
    };
    let turn = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let image = DynamicImage::from_decoder(decoder).map_err(|error| describe(&error))?;
    let (width, height) = turned_size(image.dimensions(), turn);
    let mut shown = DynamicImage::ImageRgba8(shrunk(image));
    shown.apply_orientation(turn);
    Ok(Picture {
        width,
        height,
        format: format_name(format),
        bytes: bytes.len() as u64,
        vector: false,
        exif: raw.map(exif::listed).unwrap_or_default(),
        pixels: to_bgra(shown.into_rgba8()),
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
        None => Err("This file is not an image Navo can show.".to_owned()),
    }
}
