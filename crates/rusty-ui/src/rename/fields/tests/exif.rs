use std::io::Cursor;

use exif::experimental::Writer;
use exif::{Field, In, Tag, Value};
use rusty_core::rename::FieldProvider;

use super::super::ExifFields;
use super::assert_all_safe;

const ALL: [&str; 5] = ["date", "camera", "model", "width", "height"];

fn ascii(tag: Tag, text: &str) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Ascii(vec![text.as_bytes().to_vec()]),
    }
}

fn long(tag: Tag, value: u32) -> Field {
    Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Long(vec![value]),
    }
}

fn jpeg(fields: &[Field]) -> Vec<u8> {
    let mut writer = Writer::new();
    for field in fields {
        writer.push_field(field);
    }
    let mut tiff = Cursor::new(Vec::new());
    writer.write(&mut tiff, false).unwrap();
    let tiff = tiff.into_inner();
    let length = u16::try_from(tiff.len() + 8).unwrap();
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend(length.to_be_bytes());
    out.extend(b"Exif\0\0");
    out.extend(tiff);
    out.extend([0xFF, 0xD9]);
    out
}

fn read(fields: &[Field]) -> rusty_core::rename::Values {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sample.jpg");
    std::fs::write(&path, jpeg(fields)).unwrap();
    ExifFields.read(&path)
}

#[test]
fn all_fields_are_read_from_a_jpeg() {
    let values = read(&[
        ascii(Tag::Make, "Acme"),
        ascii(Tag::Model, "Snap 3/4"),
        ascii(Tag::DateTimeOriginal, "2026:10:15 09:30:05"),
        long(Tag::PixelXDimension, 4000),
        long(Tag::PixelYDimension, 3000),
    ]);
    assert_eq!(values.get("date"), Some("2026-10-15 09.30.05"));
    assert_eq!(values.get("camera"), Some("Acme"));
    assert_eq!(values.get("model"), Some("Snap 3_4"));
    assert_eq!(values.get("width"), Some("4000"));
    assert_eq!(values.get("height"), Some("3000"));
    assert_all_safe(&values, &ALL);
}

#[test]
fn the_modified_date_is_the_fallback_for_the_taken_date() {
    let values = read(&[ascii(Tag::DateTime, "2025:01:02 03:04:05")]);
    assert_eq!(values.get("date"), Some("2025-01-02 03.04.05"));
}

#[test]
fn a_missing_field_gives_none() {
    let values = read(&[ascii(Tag::Make, "Acme")]);
    assert_eq!(values.get("camera"), Some("Acme"));
    for field in ["date", "model", "width", "height"] {
        assert_eq!(values.get(field), None, "{field}");
    }
}

#[test]
fn a_malformed_date_gives_none() {
    let values = read(&[ascii(Tag::DateTimeOriginal, "yesterday at noon")]);
    assert_eq!(values.get("date"), None);
}

#[test]
fn a_file_without_exif_gives_no_values() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("plain.jpg");
    std::fs::write(&path, [0xFF, 0xD8, 0xFF, 0xD9]).unwrap();
    let values = ExifFields.read(&path);
    for field in ALL {
        assert_eq!(values.get(field), None);
    }
    assert_eq!(
        ExifFields.read(&dir.path().join("gone.jpg")).get("date"),
        None
    );
}
