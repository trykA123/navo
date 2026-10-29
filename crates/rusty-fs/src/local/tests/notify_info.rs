use rusty_core::ProviderPath;

use super::super::notify_info::parse;
use crate::watch::RawKind;

fn record(action: u32, name: &str, last: bool) -> Vec<u8> {
    let units: Vec<u16> = name.encode_utf16().collect();
    let mut bytes = Vec::new();
    let length = (units.len() * 2) as u32;
    let size = (12 + length as usize).next_multiple_of(4);
    let next = if last { 0 } else { size as u32 };
    bytes.extend(next.to_le_bytes());
    bytes.extend(action.to_le_bytes());
    bytes.extend(length.to_le_bytes());
    for unit in units {
        bytes.extend(unit.to_le_bytes());
    }
    bytes.resize(size, 0);
    bytes
}

fn words(bytes: &[u8]) -> Vec<u32> {
    bytes
        .chunks(4)
        .map(|chunk| {
            let mut word = [0u8; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            u32::from_le_bytes(word)
        })
        .collect()
}

#[test]
fn every_record_of_a_change_buffer_is_read_in_order() {
    let mut bytes = record(1, "new.txt", false);
    bytes.extend(record(4, "old", false));
    bytes.extend(record(5, "renamed", false));
    bytes.extend(record(2, "gone", true));
    let found = parse(&words(&bytes), bytes.len());
    let expected = [
        (RawKind::Created, "new.txt"),
        (RawKind::MovedFrom(0), "old"),
        (RawKind::MovedTo(0), "renamed"),
        (RawKind::Removed, "gone"),
    ]
    .map(|(kind, name)| (kind, ProviderPath::from_text(name)));
    assert_eq!(found, expected);
}

#[test]
fn a_name_outside_the_basic_plane_survives() {
    let bytes = record(3, "nota-\u{1F3B5}.mp3", true);
    let found = parse(&words(&bytes), bytes.len());
    assert_eq!(
        found,
        [(
            RawKind::Modified,
            ProviderPath::from_text("nota-\u{1F3B5}.mp3")
        )]
    );
}

#[test]
fn a_record_cut_off_by_the_filled_length_is_ignored() {
    let mut bytes = record(1, "kept", false);
    let kept = bytes.len();
    bytes.extend(record(1, "beyond", true));
    let found = parse(&words(&bytes), kept);
    assert_eq!(found, [(RawKind::Created, ProviderPath::from_text("kept"))]);
}

#[test]
fn an_unknown_action_is_skipped() {
    let mut bytes = record(9, "odd", false);
    bytes.extend(record(1, "fine", true));
    let found = parse(&words(&bytes), bytes.len());
    assert_eq!(found, [(RawKind::Created, ProviderPath::from_text("fine"))]);
}
