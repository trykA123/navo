//! Image header checks, format names and strict decode limits for the pair diff (V37).

use std::io::Cursor;

use ::image::{DynamicImage, ImageError, ImageFormat, ImageReader, Limits};
use navo_compare::image::MAX_PIXELS;
use navo_ui::compare_diff::{ImageLimit, ImageSize};

use super::super::load::{LoadInput, Side};
use super::Failure;

/// The allocation budget of one decoded side, in bytes.
pub(super) const MAX_ALLOC: u64 = 256 * 1024 * 1024;
/// image-webp allocates outside `max_alloc`: 12 B/px peak (animated; 10.5 lossy with alpha)
/// against 9.27 B/px for a progressive JPEG, so 3/4 of D20's 64 MP keeps the same peak.
pub(super) const WEBP_MAX_PIXELS: u64 = MAX_PIXELS / 4 * 3;
/// The widest or tallest side the decoder accepts; the 64 MP cap bounds the area.
const MAX_SIDE: u32 = 64_000_000;

#[cfg(test)]
static OVERSIZE_DECODES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Number of decoders built for images over 64 MP; tests assert this stays zero.
#[cfg(test)]
pub(crate) fn oversize_decodes() -> usize {
    OVERSIZE_DECODES.load(std::sync::atomic::Ordering::SeqCst)
}

/// Reset the oversize decoder counter before one test.
#[cfg(test)]
pub(crate) fn reset_oversize_decodes() {
    OVERSIZE_DECODES.store(0, std::sync::atomic::Ordering::SeqCst);
}

/// True when an image has more than 64 million pixels (D20).
pub(crate) fn too_large(size: ImageSize) -> bool {
    u64::from(size.width) * u64::from(size.height) > MAX_PIXELS
}

/// True when a WebP is over its 48 MP cap, where image-webp would allocate unchecked.
pub(super) fn webp_over_cap(format: ImageFormat, size: ImageSize) -> bool {
    format == ImageFormat::WebP && u64::from(size.width) * u64::from(size.height) > WEBP_MAX_PIXELS
}

/// Read the pixel dimensions the decoder will report, before any decoder exists: the selected
/// ICO entry's embedded image, or `imagesize` for every other format.
pub(super) fn dimensions(bytes: &[u8], format: ImageFormat) -> Result<ImageSize, Failure> {
    if format == ImageFormat::Ico {
        return super::ico::dimensions(bytes);
    }
    match imagesize::blob_size(bytes) {
        Ok(size) => Ok(ImageSize {
            width: u32::try_from(size.width)
                .map_err(|_| Failure::TooLarge(ImageLimit::Pixels, false))?,
            height: u32::try_from(size.height)
                .map_err(|_| Failure::TooLarge(ImageLimit::Pixels, false))?,
        }),
        Err(imagesize::ImageError::NotSupported) => Err(Failure::Unsupported),
        Err(_) => Err(Failure::Decode),
    }
}

/// The strict width and height limits plus the shared allocation budget.
fn decode_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    limits
}

/// Build a decoder for one snapshot, counting oversized constructions in tests.
fn decoder(bytes: &[u8], format: ImageFormat) -> ImageReader<Cursor<&[u8]>> {
    #[cfg(test)]
    if dimensions(bytes, format).is_ok_and(too_large) {
        OVERSIZE_DECODES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(decode_limits());
    reader
}

/// Decode one image from the same snapshot under the decode limits.
pub(super) fn decode(bytes: &[u8], format: ImageFormat) -> Result<DynamicImage, Failure> {
    match decoder(bytes, format).decode() {
        Ok(image) => Ok(image),
        Err(ImageError::Unsupported(_)) => Err(Failure::Unsupported),
        Err(ImageError::Limits(_)) => Err(Failure::TooLarge(ImageLimit::Decode, false)),
        Err(_) => Err(Failure::Decode),
    }
}

/// The format the snapshot's magic bytes name, so a renamed file still gets its own decoder and
/// cap; the name's format when the bytes name none (TGA has no magic bytes). `Unsupported` when
/// neither names a raster format.
pub(super) fn format_of_snapshot(bytes: &[u8], side: &Side) -> Result<ImageFormat, Failure> {
    ::image::guess_format(bytes).or_else(|_| format_of_side(side))
}

/// The decoder format for a file name; SVG and unknown names have none.
pub(super) fn format_of(name: &str) -> Option<ImageFormat> {
    let (_, extension) = name.rsplit_once('.')?;
    let extension = extension.to_ascii_lowercase();
    match extension.as_str() {
        "jpe" => Some(ImageFormat::Jpeg),
        _ => ImageFormat::from_extension(extension),
    }
}

/// The decoder format of a side, or `Unsupported` when its name has none.
pub(super) fn format_of_side((_, entry): &Side) -> Result<ImageFormat, Failure> {
    format_of(&entry.name().to_text()).ok_or(Failure::Unsupported)
}

/// The label shown for the first side whose format the decoder does not support.
pub(super) fn unsupported_label(input: &LoadInput) -> &'static str {
    let sides = [input.left.as_ref(), input.right.as_ref()];
    for side in sides.into_iter().flatten() {
        if let Some(label) = unsupported_label_of(side) {
            return label;
        }
    }
    "image"
}

/// An SVG or SVGZ extension, which the raster decoder cannot read.
fn unsupported_label_of((_, entry): &Side) -> Option<&'static str> {
    let name = entry.name().to_text();
    let (_, extension) = name.rsplit_once('.')?;
    match extension.to_ascii_lowercase().as_str() {
        "svg" => Some("SVG"),
        "svgz" => Some("SVGZ"),
        _ => None,
    }
}
