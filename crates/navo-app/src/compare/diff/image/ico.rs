//! ICO sizing for the pair diff (V37): pick the entry the `image` ICO decoder will pick and read
//! the embedded PNG or BMP header, so the size check sees what the decoder will allocate.

use navo_ui::compare_diff::ImageSize;

use super::Failure;

const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
/// The ICO header: reserved, type and entry count, two bytes each.
const HEADER: usize = 6;
/// One directory entry, in bytes.
const ENTRY: usize = 16;
/// The decoder rejects a plane count or bit depth above this.
const MAX_FIELD: u16 = 256;
/// The smallest BMP header the decoder accepts (BITMAPCOREHEADER).
const CORE_HEADER: u32 = 12;

/// The selection key and image offset of one directory entry.
struct Entry {
    score: (u16, u32),
    offset: usize,
}

/// The size the ICO decoder will report: the selected entry's embedded image header.
/// Fails with `Decode` on a short or malformed directory or embedded header.
pub(super) fn dimensions(bytes: &[u8]) -> Result<ImageSize, Failure> {
    let entry = selected(bytes)?;
    let image = bytes.get(entry.offset..).ok_or(Failure::Decode)?;
    if image.starts_with(&PNG_SIGNATURE) {
        png_size(image)
    } else {
        bmp_size(image)
    }
}

/// The entry with the highest (bit depth, area), chosen as `image` 0.25 does: start from the
/// last entry and replace it only with a strictly higher score, in file order.
fn selected(bytes: &[u8]) -> Result<Entry, Failure> {
    let count = usize::from(u16_at(bytes, 4)?);
    let mut entries = (0..count)
        .map(|index| entry(bytes, HEADER + index * ENTRY))
        .collect::<Result<Vec<_>, _>>()?;
    let mut best = entries.pop().ok_or(Failure::Decode)?;
    for entry in entries {
        if entry.score > best.score {
            best = entry;
        }
    }
    Ok(best)
}

/// Parse one directory entry at `at`; a width or height byte of 0 means 256.
fn entry(bytes: &[u8], at: usize) -> Result<Entry, Failure> {
    let side = |byte: u8| if byte == 0 { 256 } else { u32::from(byte) };
    let width = side(*bytes.get(at).ok_or(Failure::Decode)?);
    let height = side(*bytes.get(at + 1).ok_or(Failure::Decode)?);
    let planes = u16_at(bytes, at + 4)?;
    let bits = u16_at(bytes, at + 6)?;
    if planes > MAX_FIELD || bits > MAX_FIELD {
        return Err(Failure::Decode);
    }
    let offset = usize::try_from(u32_at(bytes, at + 12)?).map_err(|_| Failure::Decode)?;
    Ok(Entry {
        score: (bits, width * height),
        offset,
    })
}

/// Width and height from an embedded PNG's IHDR chunk, which must come first.
fn png_size(image: &[u8]) -> Result<ImageSize, Failure> {
    if image.get(12..16) != Some(b"IHDR".as_slice()) {
        return Err(Failure::Decode);
    }
    let width = u32::from_be_bytes(array(image, 16)?);
    let height = u32::from_be_bytes(array(image, 20)?);
    Ok(ImageSize { width, height })
}

/// Width and height from an embedded BMP header; ICO stores twice the height for the mask.
fn bmp_size(image: &[u8]) -> Result<ImageSize, Failure> {
    let header = u32_at(image, 0)?;
    let (width, height) = if header == CORE_HEADER {
        (u32::from(u16_at(image, 4)?), u32::from(u16_at(image, 6)?))
    } else if header > CORE_HEADER {
        let width = i32::from_le_bytes(array(image, 4)?);
        let height = i32::from_le_bytes(array(image, 8)?);
        let width = u32::try_from(width).map_err(|_| Failure::Decode)?;
        if height == i32::MIN {
            return Err(Failure::Decode);
        }
        (width, height.unsigned_abs())
    } else {
        return Err(Failure::Decode);
    };
    Ok(ImageSize {
        width,
        height: height / 2,
    })
}

/// Four bytes at `at`, or `Decode` when the data is short.
fn array(bytes: &[u8], at: usize) -> Result<[u8; 4], Failure> {
    bytes
        .get(at..at + 4)
        .and_then(|slice| slice.try_into().ok())
        .ok_or(Failure::Decode)
}

/// A little-endian u16 at `at`.
fn u16_at(bytes: &[u8], at: usize) -> Result<u16, Failure> {
    bytes
        .get(at..at + 2)
        .and_then(|slice| slice.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or(Failure::Decode)
}

/// A little-endian u32 at `at`.
fn u32_at(bytes: &[u8], at: usize) -> Result<u32, Failure> {
    array(bytes, at).map(u32::from_le_bytes)
}
