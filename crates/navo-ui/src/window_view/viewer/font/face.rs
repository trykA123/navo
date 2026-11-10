//! Read font metadata and prepare privately named viewer previews.

use std::borrow::Cow;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::atomic::AtomicBool;

use skrifa::raw::{ReadError, TableProvider, tables::os2::SelectionFlags};
use skrifa::{FontRef, Tag, string::StringId};

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

/// Read the first face and glyph count; reject missing or truncated required metadata tables.
fn read_face(bytes: &[u8]) -> Result<(FontRef<'_>, u16), ReadError> {
    let face = FontRef::from_index(bytes, 0)?;
    face.head()?;
    face.hhea()?;
    let glyphs = face.maxp()?.num_glyphs();
    Ok((face, glyphs))
}

/// Read the first supported UTF-16 name for the IDs, or return none for missing or invalid names.
fn name(face: &FontRef<'_>, ids: &[StringId]) -> Option<String> {
    let table = face.name().ok()?;
    ids.iter().find_map(|id| {
        table
            .name_record()
            .iter()
            .filter(|record| record.name_id() == *id)
            .find_map(|record| {
                let unicode = record.platform_id() == 0
                    || (record.platform_id() == 3 && matches!(record.encoding_id(), 0 | 1));
                if !unicode {
                    return None;
                }
                Some(record.string(table.string_data()).ok()?.to_string())
            })
            .filter(|text| !text.trim().is_empty())
    })
}

fn hash_of(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// Build a privately named preview, returning none when its font tables cannot be rewritten.
fn preview(sfnt: &[u8], format: FontFormat) -> Option<Preview> {
    let hash = hash_of(sfnt);
    let family = format!("Navo Preview {hash:016x}");
    let postscript = format!("NavoPreview-{hash:016x}");
    let directory = match format {
        FontFormat::Collection => usize_at(sfnt, COLLECTION_FIRST_FACE)?,
        _ => 0,
    };
    let (flavor, mut tables) = sfnt::tables(sfnt, directory)?;
    let name = tables.iter_mut().find(|table| table.tag == NAME)?;
    name.data = Cow::Owned(names::rename(&name.data, &family, &postscript)?);
    let bytes = sfnt::write(flavor, tables)?;
    read_face(&bytes).ok()?;
    Some(Preview {
        family,
        bytes,
        hash,
    })
}

impl FontFace {
    /// Read viewer metadata and a bounded character grid; reject damaged or unnamed fonts.
    pub fn parse(bytes: Vec<u8>, cancel: &AtomicBool) -> Result<Self, String> {
        let format = font_format(&bytes).ok_or("This is not a font Navo can show.")?;
        let sfnt = match format {
            FontFormat::Woff => woff::to_sfnt(&bytes).ok_or("This WOFF font looks damaged.")?,
            _ => bytes,
        };
        let (face, glyphs) =
            read_face(&sfnt).map_err(|error| format!("This font looks damaged: {error}"))?;
        let os2 = face.os2().ok();
        let family = name(
            &face,
            &[StringId::TYPOGRAPHIC_FAMILY_NAME, StringId::FAMILY_NAME],
        )
        .ok_or("This font has no family name.")?;
        Ok(Self {
            family,
            style: name(
                &face,
                &[
                    StringId::TYPOGRAPHIC_SUBFAMILY_NAME,
                    StringId::SUBFAMILY_NAME,
                ],
            )
            .unwrap_or_else(|| "Regular".to_owned()),
            version: name(&face, &[StringId::VERSION_STRING]),
            format,
            glyphs,
            weight: os2.as_ref().map_or(400, |table| table.us_weight_class()),
            is_italic: os2
                .as_ref()
                .is_some_and(|table| table.fs_selection().contains(SelectionFlags::ITALIC))
                || face
                    .post()
                    .is_ok_and(|table| table.italic_angle().to_f64() != 0.0),
            chars: face
                .table_data(Tag::new(b"cmap"))
                .map(|table| cmap::grid_chars(table.as_bytes(), cancel))
                .unwrap_or_default(),
            preview: preview(&sfnt, format),
        })
    }
}
