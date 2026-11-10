//! Image fixtures for the V37 tests: small PNGs, patched headers and short BMP data.

use std::io::Cursor;

use ::image::{DynamicImage, ImageFormat, Rgb, RgbImage, Rgba, RgbaImage};

/// An 8-bit RGB PNG, dark blue except for one red pixel when `changed` is set.
pub(crate) fn png(width: u32, height: u32, changed: Option<(u32, u32)>) -> Vec<u8> {
    let mut pixels = RgbImage::from_pixel(width, height, Rgb([10, 20, 30]));
    if let Some((x, y)) = changed {
        pixels.put_pixel(x, y, Rgb([250, 0, 0]));
    }
    encode(DynamicImage::ImageRgb8(pixels))
}

/// An 8-bit RGBA PNG with every pixel at the same `alpha`.
pub(crate) fn rgba_png(width: u32, height: u32, alpha: u8) -> Vec<u8> {
    let pixels = RgbaImage::from_pixel(width, height, Rgba([10, 20, 30, alpha]));
    encode(DynamicImage::ImageRgba8(pixels))
}

/// A 1x1 PNG whose IHDR claims `width` by `height`; the pixel data stays 1x1.
pub(crate) fn png_with_header_size(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = png(1, 1, None);
    bytes[16..20].copy_from_slice(&width.to_be_bytes());
    bytes[20..24].copy_from_slice(&height.to_be_bytes());
    let crc = crc32(&bytes[12..29]);
    bytes[29..33].copy_from_slice(&crc.to_be_bytes());
    bytes
}

/// A PNG cut inside its IHDR, so the header itself is malformed.
pub(crate) fn png_truncated() -> Vec<u8> {
    let bytes = png(8, 8, None);
    bytes[..20].to_vec()
}

/// A 54-byte BMP header that declares `width` by `height` pixels but carries none.
pub(crate) fn bmp_header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(54);
    bytes.extend_from_slice(b"BM");
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&24u16.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&2_835u32.to_le_bytes());
    bytes.extend_from_slice(&2_835u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes
}

/// A BMP whose header claims `width` by `height` but whose data holds one pixel.
pub(crate) fn bmp_short_data(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = bmp_header(width, height);
    bytes.extend_from_slice(&[10, 20, 30]);
    bytes
}

fn encode(image: DynamicImage) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png).unwrap();
    bytes.into_inner()
}

/// Standard CRC-32 over one PNG chunk's type and data.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// A JPEG with 32 large APP1 segments before its frame header.
pub(crate) fn jpeg_with_metadata(changed: bool) -> Vec<u8> {
    let mut pixels = RgbImage::from_pixel(16, 16, Rgb([10, 20, 30]));
    if changed {
        pixels.put_pixel(3, 4, Rgb([250, 0, 0]));
    }
    let base = encode_as(DynamicImage::ImageRgb8(pixels), ImageFormat::Jpeg);
    let mut bytes = vec![0xFF, 0xD8];
    for _ in 0..32 {
        let payload = vec![0xAB; 65_533];
        bytes.extend_from_slice(&[0xFF, 0xE1]);
        let length = u16::try_from(payload.len() + 2).unwrap();
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(&payload);
    }
    bytes.extend_from_slice(&base[2..]);
    bytes
}

/// A GIF whose logical screen is larger than 64 MP while its only frame is 1x1.
pub(crate) fn gif_huge_screen() -> Vec<u8> {
    let pixels = RgbImage::from_pixel(1, 1, Rgb([10, 20, 30]));
    let mut bytes = encode_as(DynamicImage::ImageRgb8(pixels), ImageFormat::Gif);
    bytes[6..8].copy_from_slice(&60_000u16.to_le_bytes());
    bytes[8..10].copy_from_slice(&60_000u16.to_le_bytes());
    bytes
}

/// A JPEG whose SOF claims `width` by `height`, with no usable scan data.
pub(crate) fn jpeg_huge_header(width: u32, height: u32) -> Vec<u8> {
    let pixels = RgbImage::from_pixel(1, 1, Rgb([10, 20, 30]));
    let mut bytes = encode_as(DynamicImage::ImageRgb8(pixels), ImageFormat::Jpeg);
    let marker = bytes
        .windows(2)
        .position(|pair| pair == [0xFF, 0xC0] || pair == [0xFF, 0xC2])
        .expect("a JPEG frame header");
    bytes[marker + 5..marker + 7].copy_from_slice(&u16::try_from(height).unwrap().to_be_bytes());
    bytes[marker + 7..marker + 9].copy_from_slice(&u16::try_from(width).unwrap().to_be_bytes());
    bytes
}

/// A TIFF with the requested dimensions and an IFD after 2 MiB of padding.
pub(crate) fn tiff_with_size(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = tiff_ifd_at_end();
    let ifd = 2 * 1024 * 1024;
    bytes[ifd + 10..ifd + 14].copy_from_slice(&width.to_le_bytes());
    bytes[ifd + 22..ifd + 26].copy_from_slice(&height.to_le_bytes());
    bytes
}

/// A 1x1 TIFF whose IFD sits after 2 MiB of padding, beyond an old 1 MiB header read.
pub(crate) fn tiff_ifd_at_end() -> Vec<u8> {
    const IFD_OFFSET: u32 = 2 * 1024 * 1024;
    const BPS_OFFSET: u32 = IFD_OFFSET - 6;
    const SHORT: u16 = 3;
    const LONG: u16 = 4;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"II");
    bytes.extend_from_slice(&42u16.to_le_bytes());
    bytes.extend_from_slice(&IFD_OFFSET.to_le_bytes());
    bytes.extend_from_slice(&[10, 20, 30]);
    bytes.resize(BPS_OFFSET as usize, 0);
    bytes.extend_from_slice(&[8, 0, 8, 0, 8, 0]);
    bytes.extend_from_slice(&10u16.to_le_bytes());
    for (tag, kind, count, value) in [
        (256u16, LONG, 1u32, 1u32),
        (257, LONG, 1, 1),
        (258, SHORT, 3, BPS_OFFSET),
        (259, SHORT, 1, 1),
        (262, SHORT, 1, 2),
        (273, LONG, 1, 8),
        (277, SHORT, 1, 3),
        (278, LONG, 1, 1),
        (279, LONG, 1, 3),
        (284, SHORT, 1, 1),
    ] {
        bytes.extend_from_slice(&tag.to_le_bytes());
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(&count.to_le_bytes());
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes
}

/// A header-only WebP whose VP8X canvas claims `width` by `height`.
pub(crate) fn webp_with_size(width: u32, height: u32) -> Vec<u8> {
    let mut data = vec![0u8; 4];
    data.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
    data.extend_from_slice(&(height - 1).to_le_bytes()[..3]);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&22u32.to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(b"VP8X");
    bytes.extend_from_slice(&10u32.to_le_bytes());
    bytes.extend_from_slice(&data);
    bytes
}

/// A header-only WebP claiming more than 64 MP.
pub(crate) fn webp_huge_header() -> Vec<u8> {
    webp_with_size(16_000, 16_000)
}

/// A lossless WebP under the pixel cap, with one changed pixel when asked.
pub(crate) fn webp_lossless(changed: bool) -> Vec<u8> {
    let mut pixels = RgbImage::from_pixel(8, 8, Rgb([10, 20, 30]));
    if changed {
        pixels.put_pixel(3, 4, Rgb([250, 0, 0]));
    }
    encode_as(DynamicImage::ImageRgb8(pixels), ImageFormat::WebP)
}

/// Encode one image with the requested format.
fn encode_as(image: DynamicImage, format: ImageFormat) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, format).unwrap();
    bytes.into_inner()
}
