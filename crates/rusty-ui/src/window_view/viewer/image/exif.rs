use exif::{Exif, Field, In, Rational, Tag, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExifEntry {
    pub label: &'static str,
    pub value: String,
}

const TURNS: [&str; 8] = [
    "Normal",
    "Mirrored",
    "Turned 180°",
    "Flipped upside down",
    "Mirrored, turned 90° left",
    "Turned 90° right",
    "Mirrored, turned 90° right",
    "Turned 90° left",
];

fn field(exif: &Exif, tag: Tag) -> Option<&Field> {
    exif.get_field(tag, In::PRIMARY)
}

fn text(exif: &Exif, tag: Tag) -> Option<String> {
    let Value::Ascii(parts) = &field(exif, tag)?.value else {
        return None;
    };
    let joined = parts
        .iter()
        .map(|part| String::from_utf8_lossy(part).trim().to_owned())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Some(joined).filter(|joined| !joined.is_empty())
}

fn with_unit(exif: &Exif, tag: Tag) -> Option<String> {
    let found = field(exif, tag)?;
    Some(found.display_value().with_unit(exif).to_string())
}

fn camera(exif: &Exif) -> Option<String> {
    let model = text(exif, Tag::Model);
    let make = text(exif, Tag::Make);
    match (make, model) {
        (Some(make), Some(model)) if model.starts_with(&make) => Some(model),
        (Some(make), Some(model)) => Some(format!("{make} {model}")),
        (make, model) => model.or(make),
    }
}

fn taken(exif: &Exif) -> Option<String> {
    let stamp = text(exif, Tag::DateTimeOriginal).or_else(|| text(exif, Tag::DateTime))?;
    let (date, time) = stamp.split_once(' ').unwrap_or((&stamp, ""));
    Some(
        format!("{} {time}", date.replace(':', "-"))
            .trim()
            .to_owned(),
    )
}

fn exposure(exif: &Exif) -> Option<String> {
    let parts: Vec<String> = [Tag::ExposureTime, Tag::FNumber]
        .into_iter()
        .filter_map(|tag| with_unit(exif, tag))
        .collect();
    Some(parts.join(" · ")).filter(|joined| !joined.is_empty())
}

fn iso(exif: &Exif) -> Option<String> {
    let found = field(exif, Tag::PhotographicSensitivity)?;
    found.value.get_uint(0).map(|iso| iso.to_string())
}

fn degrees(exif: &Exif, tag: Tag) -> Option<f64> {
    let Value::Rational(parts) = &field(exif, tag)?.value else {
        return None;
    };
    let part = |at: usize| parts.get(at).map_or(0., Rational::to_f64);
    let value = part(0) + part(1) / 60. + part(2) / 3600.;
    value.is_finite().then_some(value)
}

fn hemisphere(exif: &Exif, tag: Tag, fallback: &'static str) -> String {
    text(exif, tag).unwrap_or_else(|| fallback.to_owned())
}

fn gps(exif: &Exif) -> Option<String> {
    let latitude = degrees(exif, Tag::GPSLatitude)?;
    let longitude = degrees(exif, Tag::GPSLongitude)?;
    let north = hemisphere(exif, Tag::GPSLatitudeRef, "N");
    let east = hemisphere(exif, Tag::GPSLongitudeRef, "E");
    Some(format!("{latitude:.5}° {north}, {longitude:.5}° {east}"))
}

fn orientation(exif: &Exif) -> Option<u8> {
    let found = field(exif, Tag::Orientation)?;
    found
        .value
        .get_uint(0)
        .and_then(|turn| u8::try_from(turn).ok())
        .filter(|turn| (1..=8).contains(turn))
}

fn entries(exif: &Exif, turn: Option<u8>) -> Vec<ExifEntry> {
    let turned = turn
        .and_then(|turn| TURNS.get(usize::from(turn) - 1))
        .map(|name| (*name).to_owned());
    [
        ("Camera", camera(exif)),
        ("Lens", text(exif, Tag::LensModel)),
        ("Taken", taken(exif)),
        ("Exposure", exposure(exif)),
        ("ISO", iso(exif)),
        ("Focal length", with_unit(exif, Tag::FocalLength)),
        ("GPS", gps(exif)),
        ("Orientation", turned),
    ]
    .into_iter()
    .filter_map(|(label, value)| value.map(|value| ExifEntry { label, value }))
    .collect()
}

pub(super) fn listed(raw: Vec<u8>) -> Vec<ExifEntry> {
    let Ok(exif) = exif::Reader::new().read_raw(raw) else {
        return Vec::new();
    };
    entries(&exif, orientation(&exif))
}
