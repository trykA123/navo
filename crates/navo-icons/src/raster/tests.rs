use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{ColorType, ExtendedColorType, ImageEncoder, ImageFormat, ImageReader};

use super::{MAX_SIDE, RasterError, STORED_SIDE, clean_stem, reencode, stored_name};
use crate::io::{ReadError, read_capped};

fn encode_rgba_png(width: u32, height: u32) -> Vec<u8> {
    let pixels = vec![0x80u8; (width as usize) * (height as usize) * 4];
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(&pixels, width, height, ExtendedColorType::Rgba8)
        .unwrap();
    out
}

fn crc32_ieee(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn temp_file_path(label: &str) -> PathBuf {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("navo-icons-{label}-{pid}-{id}.bin"))
}

#[test]
fn raster_refuses_dimensions_over_limit() {
    let png = encode_rgba_png(MAX_SIDE + 1, 1);
    assert!(matches!(reencode(&png), Err(RasterError::Limits)));
}

#[test]
fn raster_refuses_forged_header() {
    let mut png = encode_rgba_png(1, 1);
    assert_eq!(&png[12..16], b"IHDR");
    png[16..20].copy_from_slice(&100_000u32.to_be_bytes());
    png[20..24].copy_from_slice(&100_000u32.to_be_bytes());
    let crc = crc32_ieee(&png[12..29]);
    png[29..33].copy_from_slice(&crc.to_be_bytes());

    let result = reencode(&png);
    assert!(matches!(
        result,
        Err(RasterError::Limits | RasterError::Decode(_))
    ));
}

#[test]
fn raster_refuses_tiff() {
    let le_tiff = b"II*\0\x08\0\0\0";
    let be_tiff = b"MM\0*\0\0\0\x08";
    assert!(matches!(reencode(le_tiff), Err(RasterError::UnknownFormat)));
    assert!(matches!(reencode(be_tiff), Err(RasterError::UnknownFormat)));
}

#[test]
fn raster_output_is_png_without_metadata() {
    let rgb = vec![0x42u8; 4 * 4 * 3];
    let mut raw_jpeg = Vec::new();
    JpegEncoder::new(&mut raw_jpeg)
        .write_image(&rgb, 4, 4, ExtendedColorType::Rgb8)
        .unwrap();
    assert_eq!(&raw_jpeg[..2], &[0xFF, 0xD8]);

    let payload = b"Exif\0\0secret-camera-model";
    let seg_len = u16::try_from(payload.len() + 2).unwrap();
    let mut jpeg = Vec::with_capacity(raw_jpeg.len() + 4 + payload.len());
    jpeg.extend_from_slice(&raw_jpeg[..2]);
    jpeg.extend_from_slice(&[0xFF, 0xE1]);
    jpeg.extend_from_slice(&seg_len.to_be_bytes());
    jpeg.extend_from_slice(payload);
    jpeg.extend_from_slice(&raw_jpeg[2..]);
    assert!(jpeg.windows(4).any(|w| w == b"Exif"));

    let png = reencode(&jpeg).unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(!png.windows(4).any(|w| w == b"Exif"));
    assert!(!png.windows(19).any(|w| w == b"secret-camera-model"));
}

#[test]
fn raster_scales_long_side_to_256() {
    let png = encode_rgba_png(512, 128);
    let out = reencode(&png).unwrap();

    let reader = ImageReader::new(Cursor::new(&out))
        .with_guessed_format()
        .unwrap();
    assert_eq!(reader.format(), Some(ImageFormat::Png));
    let decoded = reader.decode().unwrap();
    assert_eq!(decoded.width(), STORED_SIDE);
    assert_eq!(decoded.height(), 64);
    assert_eq!(decoded.color(), ColorType::Rgba8);
}

#[test]
fn raster_refuses_truncated_png() {
    let png = encode_rgba_png(16, 16);
    let truncated = &png[..png.len() / 2];
    assert!(matches!(reencode(truncated), Err(RasterError::Decode(_))));
}

#[test]
fn raster_clean_stem_cases() {
    assert_eq!(clean_stem("My Icon (2)"), "my-icon-2");
    assert_eq!(clean_stem("…"), "icon");
    assert_eq!(clean_stem(&"a".repeat(40)), "a".repeat(32));
    assert_eq!(clean_stem(&format!("{} b", "a".repeat(31))), "a".repeat(31));

    let name = stored_name("My Icon (2)", b"payload", "png");
    let expected_hash = &blake3::hash(b"payload").to_hex()[..8];
    assert_eq!(name, format!("my-icon-2-{expected_hash}.png"));
}

#[test]
fn read_capped_refuses_one_byte_over() {
    let path = temp_file_path("read-capped");
    let cap = 64u64;

    fs::write(&path, vec![b'x'; 64]).unwrap();
    let ok = read_capped(&path, cap).unwrap();
    assert_eq!(ok.len(), 64);

    fs::write(&path, vec![b'x'; 65]).unwrap();
    let err = read_capped(&path, cap);
    let _ = fs::remove_file(&path);

    assert!(matches!(err, Err(ReadError::TooLarge)));
}
