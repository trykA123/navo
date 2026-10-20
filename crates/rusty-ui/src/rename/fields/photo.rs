use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use exif::{Exif, In, Tag, Value};
use rusty_core::rename::{FieldInfo, FieldProvider, Values};

static FIELDS: [FieldInfo; 5] = [
    FieldInfo::builtin("date", "rename-field-exif-date"),
    FieldInfo::builtin("camera", "rename-field-exif-camera"),
    FieldInfo::builtin("model", "rename-field-exif-model"),
    FieldInfo::builtin("width", "rename-field-exif-width"),
    FieldInfo::builtin("height", "rename-field-exif-height"),
];

pub struct ExifFields;

impl FieldProvider for ExifFields {
    fn group(&self) -> &str {
        "exif"
    }

    fn fields(&self) -> &[FieldInfo] {
        &FIELDS
    }

    fn read(&self, path: &Path) -> Values {
        let mut values = Values::default();
        let Some(exif) = open(path) else {
            return values;
        };
        let found = [
            ("date", date(&exif)),
            ("camera", text(&exif, &[Tag::Make])),
            ("model", text(&exif, &[Tag::Model])),
            (
                "width",
                number(&exif, &[Tag::PixelXDimension, Tag::ImageWidth]),
            ),
            (
                "height",
                number(&exif, &[Tag::PixelYDimension, Tag::ImageLength]),
            ),
        ];
        for (field, value) in found {
            if let Some(value) = value {
                values.insert(field, &value);
            }
        }
        values
    }
}

fn open(path: &Path) -> Option<Exif> {
    let mut reader = BufReader::new(File::open(path).ok()?);
    exif::Reader::new().read_from_container(&mut reader).ok()
}

fn text(exif: &Exif, tags: &[Tag]) -> Option<String> {
    tags.iter().find_map(|&tag| {
        let Value::Ascii(parts) = &exif.get_field(tag, In::PRIMARY)?.value else {
            return None;
        };
        let joined = parts
            .iter()
            .map(|part| String::from_utf8_lossy(part).trim().to_owned())
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        Some(joined).filter(|joined| !joined.is_empty())
    })
}

fn number(exif: &Exif, tags: &[Tag]) -> Option<String> {
    tags.iter().find_map(|&tag| {
        let value = exif.get_field(tag, In::PRIMARY)?.value.get_uint(0)?;
        Some(value.to_string())
    })
}

fn date(exif: &Exif) -> Option<String> {
    let stamp = text(exif, &[Tag::DateTimeOriginal, Tag::DateTime])?;
    let (day, time) = stamp.split_once(' ')?;
    let (day, time) = (day.replace(':', "-"), time.replace(':', "."));
    let shaped = |text: &str, pattern: &str| {
        text.len() == pattern.len()
            && text
                .chars()
                .zip(pattern.chars())
                .all(|(ch, shape)| match shape {
                    '0' => ch.is_ascii_digit(),
                    _ => ch == shape,
                })
    };
    (shaped(&day, "0000-00-00") && shaped(&time, "00.00.00")).then(|| format!("{day} {time}"))
}
