use rusty_core::ProviderPath;

use crate::watch::RawKind;

const ADDED: u32 = 1;
const REMOVED: u32 = 2;
const MODIFIED: u32 = 3;
const RENAMED_OLD_NAME: u32 = 4;
const RENAMED_NEW_NAME: u32 = 5;
const HEADER_BYTES: usize = 12;

pub(super) fn parse(buffer: &[u32], filled: usize) -> Vec<(RawKind, ProviderPath)> {
    let mut found = Vec::new();
    let mut at = 0;
    while at + HEADER_BYTES <= filled {
        let word = at / 4;
        let (Some(&next), Some(&action), Some(&length)) =
            (buffer.get(word), buffer.get(word + 1), buffer.get(word + 2))
        else {
            break;
        };
        let units = (0..length as usize / 2)
            .map(|index| unit_at(buffer, at + HEADER_BYTES + index * 2))
            .collect::<Option<Vec<u16>>>();
        if let (Some(kind), Some(units)) = (kind_of(action), units) {
            found.push((kind, ProviderPath::from_wide(&units)));
        }
        if next == 0 {
            break;
        }
        at += next as usize;
    }
    found
}

fn unit_at(buffer: &[u32], byte: usize) -> Option<u16> {
    let word = *buffer.get(byte / 4)?;
    let half = if byte.is_multiple_of(4) {
        word
    } else {
        word >> 16
    };
    Some((half & 0xFFFF) as u16)
}

fn kind_of(action: u32) -> Option<RawKind> {
    match action {
        ADDED => Some(RawKind::Created),
        REMOVED => Some(RawKind::Removed),
        MODIFIED => Some(RawKind::Modified),
        RENAMED_OLD_NAME => Some(RawKind::MovedFrom(0)),
        RENAMED_NEW_NAME => Some(RawKind::MovedTo(0)),
        _ => None,
    }
}
