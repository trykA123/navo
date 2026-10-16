use std::sync::Arc;
use std::time::SystemTime;

use proptest::prelude::*;
use rusty_core::{Location, ProviderPath};
use rusty_fs::{Attributes, EntryKind, FsEntry};
use rusty_store::CacheDb;

use super::support::{at, entry, key, listing, names};
use crate::disk::{FORMAT, ListingTable, codec};
use crate::{CacheKey, CachedListing, DISK_BUDGET};

fn table(budget: u64) -> ListingTable {
    ListingTable::new(CacheDb::open_in_memory().unwrap(), budget)
}

fn serial_key(serial: u64) -> CacheKey {
    CacheKey::new(Location::new(r"Z:\share\photos").unwrap(), Some(serial))
}

fn rich() -> CachedListing {
    let wide = [0x0066, 0xd800, 0x0067];
    let rows = vec![
        FsEntry {
            name: ProviderPath::from_text("Sub folder"),
            kind: EntryKind::Folder,
            size: 0,
            modified_ms: Some(-5),
            attributes: Attributes {
                hidden: true,
                system: false,
                readonly: true,
                link_to_folder: false,
            },
        },
        FsEntry {
            name: ProviderPath::from_wide(&wide),
            kind: EntryKind::Link,
            size: u64::MAX,
            modified_ms: Some(1_760_000_000_123),
            attributes: Attributes {
                link_to_folder: true,
                system: true,
                ..Attributes::default()
            },
        },
        FsEntry {
            name: ProviderPath::from_bytes(&[0x61, 0xff, 0x62]),
            ..entry("ignored")
        },
        entry("ș.txt"),
    ];
    CachedListing {
        rows: rows.into(),
        read_at: at(1_760_000_000),
        folder_changed: Some(at(1_700_000_000)),
        complete: false,
    }
}

#[test]
fn disk_budget_is_32_mib() {
    assert_eq!(DISK_BUDGET, 32 * 1024 * 1024);
}

#[test]
fn a_listing_round_trips_through_the_table() {
    let mut table = table(DISK_BUDGET);
    table.save(&serial_key(7), &rich()).unwrap();
    assert_eq!(table.load(&serial_key(7)), Some(rich()));
}

#[test]
fn an_empty_listing_round_trips() {
    let mut table = table(DISK_BUDGET);
    let empty = CachedListing {
        rows: Arc::from([]),
        read_at: SystemTime::UNIX_EPOCH,
        folder_changed: None,
        complete: true,
    };
    table.save(&key("a"), &empty).unwrap();
    assert_eq!(table.load(&key("a")), Some(empty));
}

#[test]
fn rows_are_stored_compressed() {
    let mut table = table(DISK_BUDGET);
    let many: Vec<String> = (0..2_000).map(|i| format!("photo-{i:05}.jpg")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    table.save(&key("a"), &listing(&many)).unwrap();
    let plain: usize = many.iter().map(|name| name.len() + 20).sum();
    assert!(table.used_bytes() < plain as u64 / 2);
}

#[test]
fn the_budget_evicts_the_least_recently_used_listing() {
    let mut probe = table(DISK_BUDGET);
    probe.save(&key("a"), &listing(&["x"])).unwrap();
    let one = probe.used_bytes();
    let mut table = table(one * 3);
    for name in ["a", "b", "c"] {
        table.save(&key(name), &listing(&["x"])).unwrap();
    }
    assert!(table.load(&key("a")).is_some());
    table.save(&key("d"), &listing(&["x"])).unwrap();
    assert!(table.load(&key("b")).is_none());
    for name in ["a", "c", "d"] {
        assert!(table.load(&key(name)).is_some(), "{name} was evicted");
    }
}

#[test]
fn a_corrupt_row_is_dropped() {
    let mut table = table(DISK_BUDGET);
    let mut value = codec::encode(&rich());
    let last = value.len() - 1;
    value[last] ^= 0x5a;
    table.put_raw(&key("a"), &value);
    assert_eq!(table.load(&key("a")), None);
    assert_eq!(table.used_bytes(), 0);
}

#[test]
fn a_truncated_row_is_dropped() {
    let mut table = table(DISK_BUDGET);
    let value = codec::encode(&rich());
    table.put_raw(&key("a"), &value[..value.len() / 2]);
    assert_eq!(table.load(&key("a")), None);
    assert_eq!(table.used_bytes(), 0);
}

#[test]
fn a_row_of_another_format_version_is_dropped() {
    let mut table = table(DISK_BUDGET);
    let mut value = codec::encode(&rich());
    assert_eq!(value[codec::FORMAT_AT], FORMAT);
    value[codec::FORMAT_AT] = FORMAT.wrapping_add(1);
    table.put_raw(&key("a"), &value);
    assert_eq!(table.load(&key("a")), None);
    assert_eq!(table.used_bytes(), 0);
}

#[test]
fn a_remapped_drive_never_matches() {
    let mut table = table(DISK_BUDGET);
    table
        .save(&serial_key(1), &listing(&["share-one"]))
        .unwrap();
    assert_eq!(table.load(&serial_key(2)), None);
    assert_eq!(
        table.load(&serial_key(1)).map(|found| names(&found)),
        Some(vec!["share-one".to_owned()])
    );
}

#[test]
fn a_key_without_a_serial_differs_from_one_with_a_serial() {
    let location = Location::new("other://a").unwrap();
    assert_ne!(
        codec::encode_key(&CacheKey::new(location.clone(), None)),
        codec::encode_key(&CacheKey::new(location, Some(0)))
    );
}

proptest! {
    #[test]
    fn decoding_any_bytes_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..256)) {
        let _ = codec::decode(&bytes);
    }

    #[test]
    fn decoding_a_damaged_row_never_panics(at in 0usize..4096, flip in 1u8..=255) {
        let mut value = codec::encode(&rich());
        let at = at % value.len();
        value[at] ^= flip;
        let _ = codec::decode(&value);
    }
}

fn reframed(plain: &[u8]) -> Vec<u8> {
    let len = u32::try_from(plain.len()).unwrap();
    codec::frame(len, &lz4_flex::block::compress(plain))
}

#[test]
fn the_format_is_2() {
    assert_eq!(FORMAT, 2);
}

#[test]
fn the_checksum_covers_the_length() {
    let mut value = codec::encode(&rich());
    value[codec::FORMAT_AT + 1] ^= 0x01;
    assert_eq!(codec::decode(&value), Err(codec::DecodeError::Checksum));
}

#[test]
fn a_length_beyond_what_the_packed_bytes_can_hold_is_refused() {
    let packed = lz4_flex::block::compress(b"tiny");
    let value = codec::frame(10_000, &packed);
    assert_eq!(codec::decode(&value), Err(codec::DecodeError::Header));
}

fn any_name() -> impl Strategy<Value = ProviderPath> {
    prop_oneof![
        any::<String>().prop_map(|text| ProviderPath::from_bytes(text.as_bytes())),
        proptest::collection::vec(any::<u16>(), 0..12)
            .prop_map(|units| ProviderPath::from_wide(&units)),
        proptest::collection::vec(any::<u8>(), 0..12)
            .prop_map(|bytes| ProviderPath::from_bytes(&bytes)),
    ]
}

fn any_row() -> impl Strategy<Value = FsEntry> {
    (
        any_name(),
        prop_oneof![
            Just(EntryKind::File),
            Just(EntryKind::Folder),
            Just(EntryKind::Link)
        ],
        any::<u64>(),
        any::<Option<i64>>(),
        any::<[bool; 4]>(),
    )
        .prop_map(|(name, kind, size, modified_ms, bits)| FsEntry {
            name,
            kind,
            size,
            modified_ms,
            attributes: Attributes {
                hidden: bits[0],
                system: bits[1],
                readonly: bits[2],
                link_to_folder: bits[3],
            },
        })
}

proptest! {
    #[test]
    fn decoding_any_framed_body_never_panics(plain in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = codec::decode(&reframed(&plain));
    }

    #[test]
    fn decoding_a_damaged_body_in_a_valid_frame_never_panics(at in 0usize..4096, flip in 1u8..=255) {
        let mut plain = codec::Body::new(&rich()).plain().to_vec();
        let at = at % plain.len();
        plain[at] ^= flip;
        let _ = codec::decode(&reframed(&plain));
    }

    #[test]
    fn any_rows_round_trip(
        rows in proptest::collection::vec(any_row(), 0..24),
        read_ms in 0u64..4_000_000_000_000,
        changed_ms in any::<Option<u32>>(),
        complete in any::<bool>(),
    ) {
        let listing = CachedListing {
            rows: rows.into(),
            read_at: at(0) + std::time::Duration::from_millis(read_ms),
            folder_changed: changed_ms.map(|ms| at(0) + std::time::Duration::from_millis(u64::from(ms))),
            complete,
        };
        prop_assert_eq!(codec::decode(&codec::encode(&listing)), Ok(listing));
    }
}
