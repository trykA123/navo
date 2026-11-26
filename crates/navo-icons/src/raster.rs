//! Security-hardened raster icon decoder, resizer, and PNG re-encoder.

use std::io::Cursor;

use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::{ExtendedColorType, ImageEncoder, ImageError, ImageFormat, ImageReader, Limits};

/// Maximum raw byte length accepted for a raster icon import (8 MiB).
pub const RASTER_CAP: u64 = 8 * 1024 * 1024;
/// Maximum pixel width or height accepted before decoding.
pub const MAX_SIDE: u32 = 4096;
/// Maximum pixel buffer allocation permitted during decode (64 MiB).
pub const MAX_ALLOC: u64 = 64 * 1024 * 1024;
/// Maximum pixel width or height of the stored PNG icon.
pub const STORED_SIDE: u32 = 256;

const MAX_STEM_LEN: usize = 32;

/// Why a raster icon could not be validated, decoded, or re-encoded.
#[derive(Debug, thiserror::Error)]
pub enum RasterError {
    /// The input bytes do not match an allowed raster format.
    #[error("unsupported or unrecognized raster format")]
    UnknownFormat,
    /// The input exceeds byte, dimension, or memory limits.
    #[error("raster image exceeds size or memory limits")]
    Limits,
    /// The decoder rejected the input bytes.
    #[error("failed to decode raster image: {0}")]
    Decode(String),
    /// Re-encoding the normalized image as PNG failed.
    #[error("failed to encode raster image: {0}")]
    Encode(String),
}

/// Decodes an untrusted raster image under strict limits and re-encodes it as a clean RGBA8 PNG.
pub fn reencode(bytes: &[u8]) -> Result<Vec<u8>, RasterError> {
    if bytes.len() as u64 > RASTER_CAP {
        return Err(RasterError::Limits);
    }
    let format = sniff_allowed_format(bytes)?;

    let mut dim_reader = ImageReader::with_format(Cursor::new(bytes), format);
    dim_reader.limits(decode_limits());
    let (width, height) = dim_reader.into_dimensions().map_err(map_decode_error)?;
    if width == 0 || height == 0 {
        return Err(RasterError::Decode("image has zero dimension".to_owned()));
    }
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(RasterError::Limits);
    }

    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(decode_limits());
    let decoded = reader.decode().map_err(map_decode_error)?;
    let scaled = if decoded.width() > STORED_SIDE || decoded.height() > STORED_SIDE {
        decoded.resize(STORED_SIDE, STORED_SIDE, FilterType::Lanczos3)
    } else {
        decoded
    };
    let rgba = scaled.into_rgba8();

    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            ExtendedColorType::Rgba8,
        )
        .map_err(|err| RasterError::Encode(err.to_string()))?;
    Ok(out)
}

/// Normalizes an icon filename stem to lowercase ASCII `[a-z0-9-]` of at most 32 bytes.
pub fn clean_stem(stem: &str) -> String {
    let mut out = String::with_capacity(stem.len().min(MAX_STEM_LEN));
    let mut pending_dash = false;
    for ch in stem.chars() {
        let lower = ch.to_ascii_lowercase();
        if !(lower.is_ascii_lowercase() || lower.is_ascii_digit()) {
            pending_dash |= !out.is_empty();
            continue;
        }
        if pending_dash && out.len() < MAX_STEM_LEN {
            out.push('-');
        }
        if out.len() >= MAX_STEM_LEN {
            break;
        }
        out.push(lower);
        pending_dash = false;
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "icon".to_owned()
    } else {
        out
    }
}

/// Builds a content-addressed stored filename `<clean_stem>-<hash8>.<ext>`.
pub fn stored_name(stem: &str, stored: &[u8], ext: &str) -> String {
    let clean = clean_stem(stem);
    let hex = blake3::hash(stored).to_hex();
    format!("{clean}-{}.{ext}", &hex[..8])
}

fn sniff_allowed_format(bytes: &[u8]) -> Result<ImageFormat, RasterError> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|err| RasterError::Decode(err.to_string()))?;
    match reader.format() {
        Some(
            format @ (ImageFormat::Png
            | ImageFormat::Jpeg
            | ImageFormat::Bmp
            | ImageFormat::Ico
            | ImageFormat::Gif
            | ImageFormat::WebP),
        ) => Ok(format),
        _ => Err(RasterError::UnknownFormat),
    }
}

fn decode_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    limits
}

fn map_decode_error(err: ImageError) -> RasterError {
    match err {
        ImageError::Limits(_) => RasterError::Limits,
        other => RasterError::Decode(other.to_string()),
    }
}

#[cfg(test)]
mod tests;
