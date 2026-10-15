use std::io::Read;

use flate2::read::ZlibDecoder;

const HEADER: usize = 44;
const ENTRY: usize = 20;
const SFNT_HEADER: usize = 12;
const RECORD: usize = 16;
const MAX_TABLE: usize = 64 * 1024 * 1024;

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    let pair = bytes.get(at..at + 2)?;
    Some(u16::from_be_bytes([pair[0], pair[1]]))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    let quad = bytes.get(at..at + 4)?;
    Some(u32::from_be_bytes([quad[0], quad[1], quad[2], quad[3]]))
}

fn usize_at(bytes: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u32_at(bytes, at)?).ok()
}

struct Table {
    tag: u32,
    checksum: u32,
    data: Vec<u8>,
}

fn table(woff: &[u8], at: usize) -> Option<Table> {
    let offset = usize_at(woff, at + 4)?;
    let packed = usize_at(woff, at + 8)?;
    let length = usize_at(woff, at + 12).filter(|length| *length <= MAX_TABLE)?;
    let stored = woff.get(offset..offset.checked_add(packed)?)?;
    let data = if packed < length {
        let mut data = Vec::new();
        ZlibDecoder::new(stored)
            .take(length as u64)
            .read_to_end(&mut data)
            .ok()?;
        data
    } else {
        stored.get(..length)?.to_vec()
    };
    (data.len() == length).then_some(Table {
        tag: u32_at(woff, at)?,
        checksum: u32_at(woff, at + 16)?,
        data,
    })
}

fn sfnt_header(flavor: u32, count: u16) -> Vec<u8> {
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

pub(super) fn to_sfnt(woff: &[u8]) -> Option<Vec<u8>> {
    let flavor = u32_at(woff, 4)?;
    let count = u16_at(woff, 12)?;
    let tables: Vec<Table> = (0..usize::from(count))
        .map(|index| table(woff, HEADER + index * ENTRY))
        .collect::<Option<_>>()?;
    let mut out = sfnt_header(flavor, count);
    let mut offset = SFNT_HEADER + tables.len() * RECORD;
    for table in &tables {
        out.extend_from_slice(&table.tag.to_be_bytes());
        out.extend_from_slice(&table.checksum.to_be_bytes());
        out.extend_from_slice(&u32::try_from(offset).ok()?.to_be_bytes());
        out.extend_from_slice(&u32::try_from(table.data.len()).ok()?.to_be_bytes());
        offset += table.data.len().next_multiple_of(4);
    }
    for table in tables {
        let padded = table.data.len().next_multiple_of(4);
        out.extend_from_slice(&table.data);
        out.resize(out.len() + padded - table.data.len(), 0);
    }
    Some(out)
}
