//! The `NVIC1` icon pack (BB-D6): a bounds-checked reader over borrowed bytes and its writer.

use std::cmp::Ordering;
use std::io::Read;
use std::sync::OnceLock;

use ruzstd::decoding::StreamingDecoder;

mod write;

pub use write::{IconDraft, SetDraft, write};

/// Why a pack or a part of it cannot be read or written.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PackError {
    /// The file does not start with `NVIC1\0`.
    #[error("not an icon pack")]
    BadMagic,
    /// The pack version is not 1.
    #[error("icon pack version {0} is not supported")]
    BadVersion(u16),
    /// The header or a set record ends early.
    #[error("icon pack is truncated")]
    Truncated,
    /// An offset, a length or a count points outside the pack.
    #[error("icon pack points outside itself")]
    OutOfRange,
    /// A chunk, tag list or body is over its cap.
    #[error("icon pack part is too large")]
    TooLarge,
    /// A zstd frame does not decode to its stated length.
    #[error("icon pack frame is damaged: {0}")]
    Zstd(String),
    /// A text field is not the allowed ASCII or UTF-8.
    #[error("icon pack text is invalid")]
    BadText,
    /// Two icons or two sets share a name.
    #[error("icon pack has `{0}` twice")]
    DuplicateName(String),
    /// The pack has no set with this id.
    #[error("icon pack has no set `{0}`")]
    NoSet(String),
}

/// What a set record says about one set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetInfo {
    /// The set id (`lucide`, `brands`).
    pub id: String,
    /// The upstream version.
    pub version: String,
    /// The licence id.
    pub licence: String,
    /// The icon grid in px.
    pub grid: u16,
    /// The number of icons.
    pub count: u32,
}

/// Where one icon's body lives: its chunk, the range inside the decoded chunk, its brand colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IconRef {
    /// The chunk index in its set.
    pub chunk: u16,
    /// The body offset inside the decoded chunk.
    pub offset: u32,
    /// The body length.
    pub len: u32,
    /// The brand colour as red, green and blue.
    pub brand: Option<[u8; 3]>,
}

/// The pack magic.
pub(crate) const MAGIC: &[u8; 6] = b"NVIC1\0";
/// The only pack version.
pub(crate) const VERSION: u16 = 1;
/// The most sets one pack holds.
pub(crate) const MAX_SETS: usize = 16;
/// Magic, version and set count.
pub(crate) const HEADER_LEN: usize = 12;
/// One set record.
pub(crate) const RECORD_LEN: usize = 80;
/// One chunk directory entry: offset, compressed length, raw length.
pub(crate) const DIR_ENTRY_LEN: usize = 12;
/// A names-table entry without its name: length byte, chunk, offset, length, colour.
pub(crate) const ENTRY_FIXED_LEN: usize = 15;
/// The colour value meaning "no brand colour".
pub(crate) const NO_BRAND: u32 = 0xFFFF_FFFF;
/// The largest decoded chunk.
pub const MAX_CHUNK_RAW: usize = 1024 * 1024;
/// The largest decoded tag list.
pub const MAX_TAGS_RAW: usize = 4 * 1024 * 1024;
/// The largest icon body.
pub const MAX_BODY: usize = 64 * 1024;
/// The largest zstd window a frame may declare, reserved before decoding: one chunk's cap. The
/// writer's frames declare 128 KiB; a per-call `cap` bound would refuse them for small chunks.
const MAX_WINDOW: u64 = 1024 * 1024;

/// Where one set's tables sit in the pack; ranges are checked on open.
struct SetPlace<'a> {
    names: &'a [u8],
    tags: &'a [u8],
    directory: &'a [u8],
    /// Start of each names-table entry, built on first lookup; empty when the table is damaged.
    index: OnceLock<Vec<usize>>,
}

/// An opened pack borrowing its bytes.
pub struct Pack<'a> {
    bytes: &'a [u8],
    sets: Vec<SetInfo>,
    places: Vec<SetPlace<'a>>,
}

impl<'a> Pack<'a> {
    /// Reads the header and the set records, and checks that every table they name is in range.
    pub fn open(bytes: &'a [u8]) -> Result<Pack<'a>, PackError> {
        let mut cursor = Cursor { bytes, at: 0 };
        if cursor.take(MAGIC.len())? != MAGIC.as_slice() {
            return Err(PackError::BadMagic);
        }
        let version = cursor.u16()?;
        if version != VERSION {
            return Err(PackError::BadVersion(version));
        }
        let count = usize::try_from(cursor.u32()?).map_err(|_| PackError::OutOfRange)?;
        if count == 0 || count > MAX_SETS {
            return Err(PackError::OutOfRange);
        }
        let mut pack = Pack {
            bytes,
            sets: Vec::with_capacity(count),
            places: Vec::with_capacity(count),
        };
        for _ in 0..count {
            let (info, place) = pack.record(cursor.take(RECORD_LEN)?)?;
            if pack.sets.iter().any(|set| set.id == info.id) {
                return Err(PackError::DuplicateName(info.id));
            }
            pack.sets.push(info);
            pack.places.push(place);
        }
        Ok(pack)
    }

    /// Every set in record order.
    pub fn sets(&self) -> &[SetInfo] {
        &self.sets
    }

    /// The icon `name` in `set`, matched exactly by a binary search on the names table.
    pub fn find(&self, set: &str, name: &str) -> Option<IconRef> {
        let place = self.place(set).ok()?;
        let index = place.index(self.count(set)?);
        let found = index
            .binary_search_by(|&at| {
                entry(place.names, at).map_or(Ordering::Less, |(key, _)| {
                    key.as_bytes().cmp(name.as_bytes())
                })
            })
            .ok()?;
        entry(place.names, *index.get(found)?).map(|(_, icon)| icon)
    }

    /// Every icon name in `set`, in table order; empty for an unknown set or a damaged table.
    pub fn names(&self, set: &str) -> Vec<&'a str> {
        let (Ok(place), Some(count)) = (self.place(set), self.count(set)) else {
            return Vec::new();
        };
        place
            .index(count)
            .iter()
            .filter_map(|&at| entry(place.names, at).map(|(name, _)| name))
            .collect()
    }

    /// Decodes chunk `chunk` of `set`; refuses a stated raw length over 1 MiB or a different decoded length.
    pub fn chunk(&self, set: &str, chunk: u16) -> Result<Vec<u8>, PackError> {
        let place = self.place(set)?;
        let start = usize::from(chunk)
            .checked_mul(DIR_ENTRY_LEN)
            .ok_or(PackError::OutOfRange)?;
        let end = start
            .checked_add(DIR_ENTRY_LEN)
            .ok_or(PackError::OutOfRange)?;
        let mut cursor = Cursor {
            bytes: place
                .directory
                .get(start..end)
                .ok_or(PackError::OutOfRange)?,
            at: 0,
        };
        let (offset, len, raw_len) = (cursor.u32()?, cursor.u32()?, cursor.u32()?);
        let raw_len = usize::try_from(raw_len).map_err(|_| PackError::TooLarge)?;
        if raw_len > MAX_CHUNK_RAW {
            return Err(PackError::TooLarge);
        }
        let decoded = decode(range(self.bytes, offset, len)?, raw_len)?;
        if decoded.len() != raw_len {
            return Err(PackError::Zstd(format!(
                "chunk decodes to {} bytes, not {raw_len}",
                decoded.len()
            )));
        }
        Ok(decoded)
    }

    /// Decodes the tag lines of `set`, one per icon in names order; refuses over 4 MiB.
    pub fn tags(&self, set: &str) -> Result<String, PackError> {
        let decoded = decode(self.place(set)?.tags, MAX_TAGS_RAW)?;
        if decoded.len() > MAX_TAGS_RAW {
            return Err(PackError::TooLarge);
        }
        String::from_utf8(decoded).map_err(|_| PackError::BadText)
    }

    /// Reads one 80-byte set record.
    fn record(&self, record: &'a [u8]) -> Result<(SetInfo, SetPlace<'a>), PackError> {
        let mut cursor = Cursor {
            bytes: record,
            at: 0,
        };
        let id = text(cursor.take(24)?)?;
        let version = text(cursor.take(12)?)?;
        let licence = text(cursor.take(12)?)?;
        // `flags` bit 0 says the set has brand colours; the reader ignores it, each names-table
        // entry carries its own colour or `NO_BRAND`.
        let (grid, _flags, count) = (cursor.u16()?, cursor.u16()?, cursor.u32()?);
        let names = range(self.bytes, cursor.u32()?, cursor.u32()?)?;
        let tags = range(self.bytes, cursor.u32()?, cursor.u32()?)?;
        let directory_at = cursor.u32()?;
        let chunks = cursor.u32()?;
        if id.is_empty() {
            return Err(PackError::BadText);
        }
        if chunks > u32::from(u16::MAX) + 1 {
            return Err(PackError::OutOfRange);
        }
        let directory_len = chunks.checked_mul(12).ok_or(PackError::OutOfRange)?;
        let directory = range(self.bytes, directory_at, directory_len)?;
        let info = SetInfo {
            id,
            version,
            licence,
            grid,
            count,
        };
        Ok((
            info,
            SetPlace {
                names,
                tags,
                directory,
                index: OnceLock::new(),
            },
        ))
    }

    fn place(&self, set: &str) -> Result<&SetPlace<'a>, PackError> {
        let at = self
            .sets
            .iter()
            .position(|info| info.id == set)
            .ok_or_else(|| PackError::NoSet(set.to_owned()))?;
        self.places
            .get(at)
            .ok_or_else(|| PackError::NoSet(set.to_owned()))
    }

    fn count(&self, set: &str) -> Option<u32> {
        self.sets
            .iter()
            .find(|info| info.id == set)
            .map(|info| info.count)
    }
}

impl SetPlace<'_> {
    fn index(&self, count: u32) -> &[usize] {
        self.index
            .get_or_init(|| build_index(self.names, count).unwrap_or_default())
    }
}

/// The start of every entry; `None` unless the table holds exactly `count` UTF-8 names in
/// strictly rising byte order, so the binary search can trust it.
fn build_index(names: &[u8], count: u32) -> Option<Vec<usize>> {
    let count = usize::try_from(count).ok()?;
    let mut starts = Vec::with_capacity(count.min(names.len() / ENTRY_FIXED_LEN));
    let mut at = 0;
    let mut last: Option<&str> = None;
    for _ in 0..count {
        let (name, _) = entry(names, at)?;
        if last.is_some_and(|last| last.as_bytes() >= name.as_bytes()) {
            return None;
        }
        starts.push(at);
        last = Some(name);
        at = at.checked_add(ENTRY_FIXED_LEN)?.checked_add(name.len())?;
    }
    (at == names.len()).then_some(starts)
}

/// Reads the names-table entry at `at`.
fn entry(names: &[u8], at: usize) -> Option<(&str, IconRef)> {
    let mut cursor = Cursor { bytes: names, at };
    let len = usize::from(cursor.u8().ok()?);
    let name = std::str::from_utf8(cursor.take(len).ok()?).ok()?;
    let (chunk, offset, len) = (cursor.u16().ok()?, cursor.u32().ok()?, cursor.u32().ok()?);
    let rgb = cursor.u32().ok()?;
    let [_, red, green, blue] = rgb.to_be_bytes();
    let brand = (rgb != NO_BRAND && rgb <= 0x00FF_FFFF).then_some([red, green, blue]);
    Some((
        name,
        IconRef {
            chunk,
            offset,
            len,
            brand,
        },
    ))
}

/// The body of `icon` inside its decoded chunk; `None` when out of range or over 64 KiB.
pub fn body<'c>(chunk: &'c [u8], icon: &IconRef) -> Option<&'c [u8]> {
    let len = usize::try_from(icon.len).ok()?;
    if len > MAX_BODY {
        return None;
    }
    let start = usize::try_from(icon.offset).ok()?;
    chunk.get(start..start.checked_add(len)?)
}

/// The slice at `offset` of `len` bytes, refused when it leaves the pack.
fn range(bytes: &[u8], offset: u32, len: u32) -> Result<&[u8], PackError> {
    let start = usize::try_from(offset).map_err(|_| PackError::OutOfRange)?;
    let len = usize::try_from(len).map_err(|_| PackError::OutOfRange)?;
    let end = start.checked_add(len).ok_or(PackError::OutOfRange)?;
    bytes.get(start..end).ok_or(PackError::OutOfRange)
}

/// Decodes one zstd frame, reading at most `cap + 1` bytes so a bomb stops at the cap; a frame
/// declaring a window over `MAX_WINDOW` is refused before the decoder reserves it.
fn decode(frame: &[u8], cap: usize) -> Result<Vec<u8>, PackError> {
    let decoder = StreamingDecoder::new_with_max_window_size(frame, MAX_WINDOW)
        .map_err(|error| PackError::Zstd(error.to_string()))?;
    let limit = u64::try_from(cap)
        .map_err(|_| PackError::TooLarge)?
        .saturating_add(1);
    let mut decoded = Vec::new();
    decoder
        .take(limit)
        .read_to_end(&mut decoded)
        .map_err(|error| PackError::Zstd(error.to_string()))?;
    Ok(decoded)
}

/// A NUL-padded ASCII field: printable characters, then only NULs.
fn text(field: &[u8]) -> Result<String, PackError> {
    let len = field
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(field.len());
    let (value, padding) = field.split_at(len);
    if !value.iter().all(u8::is_ascii_graphic) || padding.iter().any(|&byte| byte != 0) {
        return Err(PackError::BadText);
    }
    String::from_utf8(value.to_vec()).map_err(|_| PackError::BadText)
}

/// Reads little-endian integers forward through a slice; running out is `Truncated`.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Result<&'a [u8], PackError> {
        let end = self.at.checked_add(len).ok_or(PackError::Truncated)?;
        let found = self.bytes.get(self.at..end).ok_or(PackError::Truncated)?;
        self.at = end;
        Ok(found)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], PackError> {
        self.take(N)?.try_into().map_err(|_| PackError::Truncated)
    }

    fn u8(&mut self) -> Result<u8, PackError> {
        Ok(u8::from_le_bytes(self.array()?))
    }

    fn u16(&mut self) -> Result<u16, PackError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, PackError> {
        Ok(u32::from_le_bytes(self.array()?))
    }
}

#[cfg(test)]
mod tests;
