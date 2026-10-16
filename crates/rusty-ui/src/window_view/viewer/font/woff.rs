use std::borrow::Cow;
use std::io::Read;

use flate2::read::ZlibDecoder;

use super::sfnt::{self, MAX_FONT, MAX_TABLES, Table, u16_at, u32_at, usize_at};

const HEADER: usize = 44;
const ENTRY: usize = 20;
const TOTAL_SFNT_SIZE: usize = 16;

fn table(woff: &[u8], at: usize) -> Option<Table<'_>> {
    let offset = usize_at(woff, at + 4)?;
    let packed = usize_at(woff, at + 8)?;
    let length = usize_at(woff, at + 12)?;
    let stored = woff.get(offset..offset.checked_add(packed)?)?;
    let data = if packed < length {
        let mut data = Vec::new();
        ZlibDecoder::new(stored)
            .take(length as u64)
            .read_to_end(&mut data)
            .ok()?;
        Cow::Owned(data)
    } else {
        Cow::Borrowed(stored.get(..length)?)
    };
    (data.len() == length).then_some(Table {
        tag: u32_at(woff, at)?,
        data,
    })
}

fn declared_total(woff: &[u8], count: usize) -> Option<usize> {
    (0..count).try_fold(0usize, |sum, index| {
        let length = usize_at(woff, HEADER + index * ENTRY + 12)?;
        sum.checked_add(length.next_multiple_of(4))
    })
}

pub(super) fn to_sfnt(woff: &[u8]) -> Option<Vec<u8>> {
    let flavor = u32_at(woff, 4)?;
    let count = u16_at(woff, 12).filter(|count| *count <= MAX_TABLES)?;
    let count = usize::from(count);
    usize_at(woff, TOTAL_SFNT_SIZE).filter(|size| *size <= MAX_FONT)?;
    declared_total(woff, count).filter(|total| *total <= MAX_FONT)?;
    let tables = (0..count)
        .map(|index| table(woff, HEADER + index * ENTRY))
        .collect::<Option<Vec<_>>>()?;
    sfnt::write(flavor, tables)
}
