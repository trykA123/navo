//! Builds an `NVIC1` pack from icon drafts: sorted names, chunks of up to 64 bodies, zstd frames.

use ruzstd::encoding::{CompressionLevel, compress_to_vec};

use super::{
    DIR_ENTRY_LEN, HEADER_LEN, MAGIC, MAX_BODY, MAX_CHUNK_RAW, MAX_SETS, MAX_TAGS_RAW, NO_BRAND,
    PackError, RECORD_LEN, VERSION,
};

/// One icon to pack: its name, its normalised SVG body, its search words and its brand colour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconDraft {
    /// The icon name, unique in its set (1–255 bytes).
    pub name: String,
    /// The SVG document (at most 64 KiB).
    pub svg: Vec<u8>,
    /// Search words, each without whitespace.
    pub tags: Vec<String>,
    /// The brand colour as red, green and blue.
    pub brand: Option<[u8; 3]>,
}

/// One set to pack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetDraft {
    /// The set id (1–24 ASCII characters).
    pub id: String,
    /// The upstream version (at most 12 ASCII characters).
    pub version: String,
    /// The licence id (at most 12 ASCII characters).
    pub licence: String,
    /// The icon grid in px.
    pub grid: u16,
    /// The icons, in any order.
    pub icons: Vec<IconDraft>,
}

/// Writes the sets as one pack; the same drafts always give the same bytes.
///
/// Icons are sorted by name bytes and cut into chunks of at most 64 bodies and 1 MiB raw.
pub fn write(sets: &[SetDraft]) -> Result<Vec<u8>, PackError> {
    if sets.is_empty() || sets.len() > MAX_SETS {
        return Err(PackError::OutOfRange);
    }
    for (at, set) in sets.iter().enumerate() {
        if sets.iter().skip(at + 1).any(|other| other.id == set.id) {
            return Err(PackError::DuplicateName(set.id.clone()));
        }
    }
    let built = sets.iter().map(build).collect::<Result<Vec<_>, _>>()?;
    let records_len = RECORD_LEN
        .checked_mul(sets.len())
        .ok_or(PackError::TooLarge)?;
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&offset(sets.len())?.to_le_bytes());
    out.resize(HEADER_LEN + records_len, 0);
    for (at, (set, built)) in sets.iter().zip(&built).enumerate() {
        let record = place(&mut out, set, built)?;
        let start = HEADER_LEN + at * RECORD_LEN;
        out.get_mut(start..start + RECORD_LEN)
            .ok_or(PackError::OutOfRange)?
            .copy_from_slice(&record);
    }
    offset(out.len())?;
    Ok(out)
}

/// One set turned into bytes, before it has a place in the pack.
struct Built {
    names: Vec<u8>,
    tags: Vec<u8>,
    /// Each chunk's zstd frame and its raw length.
    chunks: Vec<(Vec<u8>, u32)>,
    count: u32,
    has_brand: bool,
}

/// Appends a set's names table, chunk directory, tags and chunks, and returns its record.
fn place(out: &mut Vec<u8>, set: &SetDraft, built: &Built) -> Result<[u8; RECORD_LEN], PackError> {
    let names_at = offset(out.len())?;
    out.extend_from_slice(&built.names);
    let directory_at = offset(out.len())?;
    let directory_len = DIR_ENTRY_LEN
        .checked_mul(built.chunks.len())
        .ok_or(PackError::TooLarge)?;
    let tags_at = offset(
        out.len()
            .checked_add(directory_len)
            .ok_or(PackError::TooLarge)?,
    )?;
    let mut frame_at = tags_at
        .checked_add(offset(built.tags.len())?)
        .ok_or(PackError::TooLarge)?;
    for (frame, raw_len) in &built.chunks {
        let len = offset(frame.len())?;
        for value in [frame_at, len, *raw_len] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        frame_at = frame_at.checked_add(len).ok_or(PackError::TooLarge)?;
    }
    out.extend_from_slice(&built.tags);
    for (frame, _) in &built.chunks {
        out.extend_from_slice(frame);
    }
    let mut record = Vec::with_capacity(RECORD_LEN);
    record.extend_from_slice(&fixed::<24>(&set.id)?);
    record.extend_from_slice(&fixed::<12>(&set.version)?);
    record.extend_from_slice(&fixed::<12>(&set.licence)?);
    record.extend_from_slice(&set.grid.to_le_bytes());
    record.extend_from_slice(&u16::from(built.has_brand).to_le_bytes());
    let chunk_count = offset(built.chunks.len())?;
    let fields = [
        built.count,
        names_at,
        offset(built.names.len())?,
        tags_at,
        offset(built.tags.len())?,
        directory_at,
        chunk_count,
    ];
    for value in fields {
        record.extend_from_slice(&value.to_le_bytes());
    }
    record.try_into().map_err(|_| PackError::OutOfRange)
}

/// Sorts, checks and chunks one set's icons.
fn build(set: &SetDraft) -> Result<Built, PackError> {
    if !is_set_id(&set.id) {
        return Err(PackError::BadText);
    }
    fixed::<12>(&set.version)?;
    fixed::<12>(&set.licence)?;
    let mut icons: Vec<&IconDraft> = set.icons.iter().collect();
    icons.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
    if let Some(pair) = icons.windows(2).find(|pair| pair[0].name == pair[1].name) {
        return Err(PackError::DuplicateName(pair[0].name.clone()));
    }
    let mut built = Built {
        names: Vec::new(),
        tags: Vec::new(),
        chunks: Vec::new(),
        count: offset(icons.len())?,
        has_brand: false,
    };
    let mut raw = Vec::new();
    let mut in_chunk = 0;
    let mut tags = String::new();
    for icon in icons {
        if icon.name.is_empty()
            || icon
                .tags
                .iter()
                .any(|tag| tag.is_empty() || tag.contains(char::is_whitespace))
        {
            return Err(PackError::BadText);
        }
        if icon.svg.len() > MAX_BODY {
            return Err(PackError::TooLarge);
        }
        if in_chunk == ICONS_PER_CHUNK || raw.len() + icon.svg.len() > MAX_CHUNK_RAW {
            built.chunks.push(compress(&raw)?);
            raw.clear();
            in_chunk = 0;
        }
        let chunk = u16::try_from(built.chunks.len()).map_err(|_| PackError::TooLarge)?;
        push_entry(&mut built.names, icon, chunk, offset(raw.len())?)?;
        built.has_brand |= icon.brand.is_some();
        raw.extend_from_slice(&icon.svg);
        in_chunk += 1;
        tags.push_str(&icon.tags.join(" "));
        tags.push('\n');
    }
    if in_chunk > 0 {
        built.chunks.push(compress(&raw)?);
    }
    if tags.len() > MAX_TAGS_RAW {
        return Err(PackError::TooLarge);
    }
    built.tags = compress_to_vec(tags.as_bytes(), CompressionLevel::Fastest);
    Ok(built)
}

/// Appends one names-table entry: name, chunk, offset, length and colour.
fn push_entry(names: &mut Vec<u8>, icon: &IconDraft, chunk: u16, at: u32) -> Result<(), PackError> {
    let name_len = u8::try_from(icon.name.len()).map_err(|_| PackError::BadText)?;
    let rgb = icon.brand.map_or(NO_BRAND, |[red, green, blue]| {
        u32::from_be_bytes([0, red, green, blue])
    });
    names.push(name_len);
    names.extend_from_slice(icon.name.as_bytes());
    names.extend_from_slice(&chunk.to_le_bytes());
    for value in [at, offset(icon.svg.len())?, rgb] {
        names.extend_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

/// Icon bodies in one chunk at most.
const ICONS_PER_CHUNK: usize = 64;

/// One zstd frame with its raw length.
fn compress(raw: &[u8]) -> Result<(Vec<u8>, u32), PackError> {
    Ok((
        compress_to_vec(raw, CompressionLevel::Fastest),
        offset(raw.len())?,
    ))
}

/// A size or offset as the pack's `u32`.
fn offset(value: usize) -> Result<u32, PackError> {
    u32::try_from(value).map_err(|_| PackError::TooLarge)
}

/// The set id grammar of BB-D5: `^[a-z][a-z0-9-]{0,23}$`.
fn is_set_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    bytes.next().is_some_and(|first| first.is_ascii_lowercase())
        && id.len() <= 24
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// A printable ASCII field NUL-padded to `N` bytes.
fn fixed<const N: usize>(text: &str) -> Result<[u8; N], PackError> {
    if text.len() > N || !text.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err(PackError::BadText);
    }
    let mut field = [0; N];
    field
        .get_mut(..text.len())
        .ok_or(PackError::BadText)?
        .copy_from_slice(text.as_bytes());
    Ok(field)
}
