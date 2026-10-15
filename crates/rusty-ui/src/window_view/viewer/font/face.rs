use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use ttf_parser::{Face, name_id};

use super::woff;

const GRID_CHARS: usize = 256;
const MIN_TABLES: u16 = 1;
const MAX_TABLES: u16 = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontFormat {
    TrueType,
    OpenType,
    Collection,
    Woff,
}

impl FontFormat {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::TrueType => "TrueType",
            Self::OpenType => "OpenType (CFF)",
            Self::Collection => "TrueType collection · first face",
            Self::Woff => "WOFF",
        }
    }
}

fn plausible_sfnt(head: &[u8]) -> bool {
    let tables = head
        .get(4..6)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]));
    let tag = head.get(12..16);
    tables.is_some_and(|count| (MIN_TABLES..=MAX_TABLES).contains(&count))
        && tag.is_some_and(|tag| {
            tag.iter()
                .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
        })
}

#[must_use]
pub fn font_format(head: &[u8]) -> Option<FontFormat> {
    match head.get(..4)? {
        b"\x00\x01\x00\x00" | b"true" if plausible_sfnt(head) => Some(FontFormat::TrueType),
        b"OTTO" if plausible_sfnt(head) => Some(FontFormat::OpenType),
        b"ttcf" => Some(FontFormat::Collection),
        b"wOFF" => Some(FontFormat::Woff),
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct FontFace {
    pub family: String,
    pub style: String,
    pub version: Option<String>,
    pub format: FontFormat,
    pub glyphs: u16,
    pub weight: u16,
    pub is_italic: bool,
    pub chars: Vec<char>,
    pub(super) sfnt: Arc<Vec<u8>>,
    pub(super) hash: u64,
}

fn name(face: &Face<'_>, ids: &[u16]) -> Option<String> {
    ids.iter().find_map(|id| {
        face.names()
            .into_iter()
            .filter(|name| name.name_id == *id)
            .find_map(|name| name.to_string())
            .filter(|text| !text.trim().is_empty())
    })
}

fn grid_chars(face: &Face<'_>) -> Vec<char> {
    let mut points = Vec::new();
    for subtable in face
        .tables()
        .cmap
        .into_iter()
        .flat_map(|cmap| cmap.subtables)
    {
        if subtable.is_unicode() {
            subtable.codepoints(|point| points.push(point));
        }
    }
    points.sort_unstable();
    points.dedup();
    points
        .into_iter()
        .filter_map(char::from_u32)
        .filter(|ch| !ch.is_control() && !ch.is_whitespace())
        .take(GRID_CHARS)
        .collect()
}

impl FontFace {
    pub fn parse(bytes: Vec<u8>) -> Result<Self, String> {
        let format = font_format(&bytes).ok_or("This is not a font Ferrite can show.")?;
        let sfnt = match format {
            FontFormat::Woff => woff::to_sfnt(&bytes).ok_or("This WOFF font looks damaged.")?,
            _ => bytes,
        };
        let face =
            Face::parse(&sfnt, 0).map_err(|error| format!("This font looks damaged: {error}"))?;
        let family = name(&face, &[name_id::TYPOGRAPHIC_FAMILY, name_id::FAMILY])
            .ok_or("This font has no family name.")?;
        let parsed = Self {
            family,
            style: name(&face, &[name_id::TYPOGRAPHIC_SUBFAMILY, name_id::SUBFAMILY])
                .unwrap_or_else(|| "Regular".to_owned()),
            version: name(&face, &[name_id::VERSION]),
            format,
            glyphs: face.number_of_glyphs(),
            weight: face.weight().to_number(),
            is_italic: face.is_italic(),
            chars: grid_chars(&face),
            sfnt: Arc::default(),
            hash: 0,
        };
        let mut hasher = DefaultHasher::new();
        sfnt.hash(&mut hasher);
        Ok(Self {
            hash: hasher.finish(),
            sfnt: Arc::new(sfnt),
            ..parsed
        })
    }
}
