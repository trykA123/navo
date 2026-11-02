use std::collections::HashMap;

use super::sfnt::u16_at;

const HEADER: usize = 6;
const RECORD: usize = 12;
const FAMILY: u16 = 1;
const FULL_NAME: u16 = 4;
const POSTSCRIPT: u16 = 6;
const TYPOGRAPHIC_FAMILY: u16 = 16;
const WWS_FAMILY: u16 = 21;
const REPLACED: [u16; 5] = [
    FAMILY,
    FULL_NAME,
    POSTSCRIPT,
    TYPOGRAPHIC_FAMILY,
    WWS_FAMILY,
];
const WINDOWS: u16 = 3;
const UNICODE_BMP: u16 = 1;
const US_ENGLISH: u16 = 0x0409;
const FIRST_LANGUAGE_TAG: u16 = 0x8000;

struct Record<'a> {
    key: [u16; 4],
    text: &'a [u8],
}

fn records(table: &[u8]) -> Option<Vec<Record<'_>>> {
    let count = usize::from(u16_at(table, 2)?);
    let storage = usize::from(u16_at(table, 4)?);
    (0..count)
        .map(|index| {
            let at = HEADER + index * RECORD;
            let field = |offset: usize| u16_at(table, at + offset);
            let start = storage + usize::from(field(10)?);
            let text = table.get(start..start + usize::from(field(8)?))?;
            Some(Record {
                key: [field(0)?, field(2)?, field(4)?, field(6)?],
                text,
            })
        })
        .collect()
}

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

fn write(records: &[Record<'_>]) -> Option<Vec<u8>> {
    let storage = HEADER + records.len() * RECORD;
    let mut out = Vec::with_capacity(storage);
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&u16::try_from(records.len()).ok()?.to_be_bytes());
    out.extend_from_slice(&u16::try_from(storage).ok()?.to_be_bytes());
    let mut strings = Vec::new();
    let mut placed: HashMap<&[u8], u16> = HashMap::new();
    for record in records {
        let offset = if let Some(offset) = placed.get(record.text) {
            *offset
        } else {
            let offset = u16::try_from(strings.len()).ok()?;
            strings.extend_from_slice(record.text);
            placed.insert(record.text, offset);
            offset
        };
        for value in record.key {
            out.extend_from_slice(&value.to_be_bytes());
        }
        out.extend_from_slice(&u16::try_from(record.text.len()).ok()?.to_be_bytes());
        out.extend_from_slice(&offset.to_be_bytes());
    }
    out.extend_from_slice(&strings);
    Some(out)
}

pub(super) fn rename(table: &[u8], family: &str, postscript: &str) -> Option<Vec<u8>> {
    let family = utf16(family);
    let postscript = utf16(postscript);
    let mut kept = records(table)?;
    kept.retain(|record| !REPLACED.contains(&record.key[3]) && record.key[2] < FIRST_LANGUAGE_TAG);
    let added = [
        (FAMILY, &family),
        (FULL_NAME, &family),
        (POSTSCRIPT, &postscript),
        (TYPOGRAPHIC_FAMILY, &family),
    ];
    kept.extend(added.map(|(id, text)| Record {
        key: [WINDOWS, UNICODE_BMP, US_ENGLISH, id],
        text,
    }));
    kept.sort_by_key(|record| record.key);
    write(&kept)
}
