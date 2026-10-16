use std::borrow::Cow;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::AtomicBool;

use ttf_parser::{Face, Tag, name_id};

use super::sfnt::{self, MAX_TABLES, usize_at};
use super::{cmap, names, woff};

const MIN_TABLES: u16 = 1;
const NAME: u32 = u32::from_be_bytes(*b"name");
const COLLECTION_FIRST_FACE: usize = 12;

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
pub(super) struct Preview {
    pub family: String,
    pub bytes: Vec<u8>,
    pub hash: u64,
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
    pub(super) preview: Option<Preview>,
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

fn hash_of(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn preview(sfnt: &[u8], format: FontFormat) -> Option<Preview> {
    let hash = hash_of(sfnt);
    let family = format!("Ferrite Preview {hash:016x}");
    let postscript = format!("FerritePreview-{hash:016x}");
    let directory = match format {
        FontFormat::Collection => usize_at(sfnt, COLLECTION_FIRST_FACE)?,
        _ => 0,
    };
    let (flavor, mut tables) = sfnt::tables(sfnt, directory)?;
    let name = tables.iter_mut().find(|table| table.tag == NAME)?;
    name.data = Cow::Owned(names::rename(&name.data, &family, &postscript)?);
    let bytes = sfnt::write(flavor, tables)?;
    Face::parse(&bytes, 0).ok()?;
    Some(Preview {
        family,
        bytes,
        hash,
    })
}

impl FontFace {
    pub fn parse(bytes: Vec<u8>, cancel: &AtomicBool) -> Result<Self, String> {
        let format = font_format(&bytes).ok_or("This is not a font Ferrite can show.")?;
        let sfnt = match format {
            FontFormat::Woff => woff::to_sfnt(&bytes).ok_or("This WOFF font looks damaged.")?,
            _ => bytes,
        };
        let face =
            Face::parse(&sfnt, 0).map_err(|error| format!("This font looks damaged: {error}"))?;
        let family = name(&face, &[name_id::TYPOGRAPHIC_FAMILY, name_id::FAMILY])
            .ok_or("This font has no family name.")?;
        Ok(Self {
            family,
            style: name(&face, &[name_id::TYPOGRAPHIC_SUBFAMILY, name_id::SUBFAMILY])
                .unwrap_or_else(|| "Regular".to_owned()),
            version: name(&face, &[name_id::VERSION]),
            format,
            glyphs: face.number_of_glyphs(),
            weight: face.weight().to_number(),
            is_italic: face.is_italic(),
            chars: face
                .raw_face()
                .table(Tag::from_bytes(b"cmap"))
                .map(|table| cmap::grid_chars(table, cancel))
                .unwrap_or_default(),
            preview: preview(&sfnt, format),
        })
    }
}
