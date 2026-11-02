use std::borrow::Cow;

pub(super) const MAX_TABLES: u16 = 128;
pub(super) const MAX_FONT: usize = 64 * 1024 * 1024;
const HEADER: usize = 12;
const RECORD: usize = 16;
const HEAD: u32 = u32::from_be_bytes(*b"head");
const ADJUSTMENT: std::ops::Range<usize> = 8..12;
const CHECKSUM_MAGIC: u32 = 0xB1B0_AFBA;

pub(super) struct Table<'a> {
    pub tag: u32,
    pub data: Cow<'a, [u8]>,
}

pub(super) fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let pair = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_be_bytes([pair[0], pair[1]]))
}

pub(super) fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let quad = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes([quad[0], quad[1], quad[2], quad[3]]))
}

pub(super) fn usize_at(bytes: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u32_at(bytes, at)?).ok()
}

pub(super) fn tables(font: &[u8], directory: usize) -> Option<(u32, Vec<Table<'_>>)> {
    let flavor = u32_at(font, directory)?;
    let count = u16_at(font, directory.checked_add(4)?)?;
    if count > MAX_TABLES {
        return None;
    }
    let tables = (0..usize::from(count))
        .map(|index| {
            let record = directory + HEADER + index * RECORD;
            let offset = usize_at(font, record + 8)?;
            let length = usize_at(font, record + 12)?;
            let data = font.get(offset..offset.checked_add(length)?)?;
            Some(Table {
                tag: u32_at(font, record)?,
                data: Cow::Borrowed(data),
            })
        })
        .collect::<Option<_>>()?;
    Some((flavor, tables))
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0, |sum: u32, chunk| {
        let mut quad = [0; 4];
        quad[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(quad))
    })
}

fn table_checksum(table: &Table<'_>) -> u32 {
    if table.tag != HEAD || table.data.len() < ADJUSTMENT.end {
        return checksum(&table.data);
    }
    let mut head = table.data.to_vec();
    head[ADJUSTMENT].fill(0);
    checksum(&head)
}

fn header(flavor: u32, count: u16) -> Vec<u8> {
    let power = if count == 0 {
        0
    } else {
        15 - count.leading_zeros() as u16
    };
    let range = (1u16 << power).saturating_mul(16);
    let mut out = Vec::new();
    out.extend_from_slice(&flavor.to_be_bytes());
    out.extend_from_slice(&count.to_be_bytes());
    out.extend_from_slice(&range.to_be_bytes());
    out.extend_from_slice(&power.to_be_bytes());
    out.extend_from_slice(&count.saturating_mul(16).saturating_sub(range).to_be_bytes());
    out
}

pub(super) fn write(flavor: u32, mut tables: Vec<Table<'_>>) -> Option<Vec<u8>> {
    tables.sort_by_key(|table| table.tag);
    let total = tables
        .iter()
        .try_fold(HEADER + tables.len() * RECORD, |sum, table| {
            sum.checked_add(table.data.len().next_multiple_of(4))
        });
    if total.is_none_or(|total| total > MAX_FONT) {
        return None;
    }
    let mut out = header(flavor, u16::try_from(tables.len()).ok()?);
    let mut offset = HEADER + tables.len() * RECORD;
    let mut head_at = None;
    for table in &tables {
        if table.tag == HEAD && table.data.len() >= ADJUSTMENT.end {
            head_at = Some(offset);
        }
        out.extend_from_slice(&table.tag.to_be_bytes());
        out.extend_from_slice(&table_checksum(table).to_be_bytes());
        out.extend_from_slice(&u32::try_from(offset).ok()?.to_be_bytes());
        out.extend_from_slice(&u32::try_from(table.data.len()).ok()?.to_be_bytes());
        offset += table.data.len().next_multiple_of(4);
    }
    for table in &tables {
        out.extend_from_slice(&table.data);
        out.resize(out.len().next_multiple_of(4), 0);
    }
    if let Some(at) = head_at {
        let adjustment = at + ADJUSTMENT.start..at + ADJUSTMENT.end;
        out[adjustment.clone()].fill(0);
        let fixed = CHECKSUM_MAGIC.wrapping_sub(checksum(&out));
        out[adjustment].copy_from_slice(&fixed.to_be_bytes());
    }
    Some(out)
}
