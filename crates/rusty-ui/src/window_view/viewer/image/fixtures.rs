use std::io::Cursor;

use exif::experimental::Writer;
use exif::{Field, In, Rational, Tag, Value};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, Rgba, RgbaImage};

pub const RED: [u8; 3] = [220, 30, 30];
pub const BLUE: [u8; 3] = [30, 30, 220];

pub fn halves(width: u32, height: u32) -> RgbaImage {
    RgbaImage::from_fn(width, height, |x, _| {
        let [r, g, b] = if x < width / 2 { RED } else { BLUE };
        Rgba([r, g, b, 255])
    })
}

pub fn encoded(format: ImageFormat, width: u32, height: u32) -> Vec<u8> {
    let picture = DynamicImage::ImageRgba8(halves(width, height));
    let picture = match format {
        ImageFormat::Jpeg | ImageFormat::Bmp => DynamicImage::ImageRgb8(picture.to_rgb8()),
        _ => picture,
    };
    let mut out = Cursor::new(Vec::new());
    picture.write_to(&mut out, format).expect("encode");
    out.into_inner()
}

fn ascii(tag: Tag, text: &str) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Ascii(vec![text.as_bytes().to_vec()]),
    }
}

fn rational(tag: Tag, parts: &[(u32, u32)]) -> Field {
    let parts = parts
        .iter()
        .map(|&(num, denom)| Rational { num, denom })
        .collect();
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Rational(parts),
    }
}

fn short(tag: Tag, value: u16) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Short(vec![value]),
    }
}

pub fn exif_block(orientation: u16) -> Vec<u8> {
    let fields = [
        ascii(Tag::Make, "FUJIFILM"),
        ascii(Tag::Model, "X-T5"),
        ascii(Tag::LensModel, "XF23mmF1.4 R LM WR"),
        ascii(Tag::DateTimeOriginal, "2026:10:15 09:30:00"),
        rational(Tag::ExposureTime, &[(1, 250)]),
        rational(Tag::FNumber, &[(14, 10)]),
        short(Tag::PhotographicSensitivity, 400),
        rational(Tag::FocalLength, &[(23, 1)]),
        ascii(Tag::GPSLatitudeRef, "N"),
        rational(Tag::GPSLatitude, &[(44, 1), (25, 1), (3600, 100)]),
        ascii(Tag::GPSLongitudeRef, "E"),
        rational(Tag::GPSLongitude, &[(26, 1), (6, 1), (0, 1)]),
        short(Tag::Orientation, orientation),
    ];
    let mut writer = Writer::new();
    for field in &fields {
        writer.push_field(field);
    }
    let mut out = Cursor::new(Vec::new());
    writer.write(&mut out, false).expect("exif");
    out.into_inner()
}

pub fn photo(width: u32, height: u32, orientation: u16) -> Vec<u8> {
    let pixels = DynamicImage::ImageRgba8(halves(width, height)).to_rgb8();
    let mut out = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut out, 95);
    encoder
        .set_exif_metadata(exif_block(orientation))
        .expect("exif");
    encoder
        .write_image(pixels.as_raw(), width, height, ExtendedColorType::Rgb8)
        .expect("jpeg");
    out
}

pub const BADGE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12" viewBox="0 0 24 12"><rect width="12" height="12" fill="#dc1e1e"/></svg>"##;
