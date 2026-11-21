//! Tests of the journal record codec: round trips, checksums, torn tails and damage.

use crate::record::{ItemMode, JobId, JobKind, Log, Record, decode, encode};
use navo_core::FileStamp;
use proptest::prelude::*;

fn stamp(seed: u8) -> FileStamp {
    FileStamp {
        size: u64::from(seed) * 1000,
        modified_ms: Some(1_763_000_000_000 + i64::from(seed)),
        hash: [seed; 32],
    }
}

/// One record of every variant, in the order a job writes them.
pub(super) fn every_variant() -> Vec<Record> {
    let mut records = vec![
        Record::Begin {
            job: JobId::new(1_763_000_000_000, 4242, 7),
            kind: JobKind::Copy,
            label: "Copy a.txt".into(),
            created_ms: 1_763_000_000_000,
            reverts: None,
            owner: "4242-1763000000000".into(),
        },
        Record::Begin {
            job: JobId::new(5, 1, 0),
            kind: JobKind::Undo,
            label: "Undo".into(),
            created_ms: 5,
            reverts: Some(JobId::new(1, 2, 3)),
            owner: "1-5".into(),
        },
    ];
    records.extend(intents());
    records.extend([
        Record::Staged {
            item: 0,
            new: stamp(1),
        },
        Record::Backed {
            item: 0,
            old: stamp(2),
        },
        Record::Done { item: 0 },
        Record::Commit,
        Record::Aborted {
            reason: "recovered".into(),
        },
    ]);
    records
}

/// One `Intent` of every mode, with edge values in the optional fields.
fn intents() -> [Record; 3] {
    [
        Record::Intent {
            item: 0,
            target: "/home/admin/a.txt".into(),
            mode: ItemMode::Overwrite,
            expect_size: Some(12),
            expect_mtime_ms: Some(-3),
            restore_mtime_ms: None,
        },
        Record::Intent {
            item: 1,
            target: r"C:\Users\admin\b.txt".into(),
            mode: ItemMode::Create,
            expect_size: None,
            expect_mtime_ms: None,
            restore_mtime_ms: Some(99),
        },
        Record::Intent {
            item: u32::MAX,
            target: "/x".into(),
            mode: ItemMode::Remove,
            expect_size: Some(u64::MAX),
            expect_mtime_ms: Some(i64::MIN),
            restore_mtime_ms: Some(i64::MAX),
        },
    ]
}

pub(super) fn log_of(records: &[Record]) -> String {
    records.iter().map(encode).collect()
}

/// The log `decode` must return for these records written whole, with no bad line.
pub(super) fn clean(records: Vec<Record>) -> Log {
    Log {
        good_len: log_of(&records).len(),
        records,
        torn_tail: false,
        damaged: false,
    }
}

#[test]
fn codec_round_trips_every_variant() {
    for record in every_variant() {
        let line = encode(&record);
        assert!(
            line.ends_with('\n'),
            "a line must end in a newline: {line:?}"
        );
        assert_eq!(
            line.matches('\n').count(),
            1,
            "one record is one line: {line:?}"
        );
        assert_eq!(decode(line.as_bytes()), clean(vec![record]));
    }
    assert_eq!(
        decode(log_of(&every_variant()).as_bytes()),
        clean(every_variant())
    );
}

#[test]
fn codec_line_is_json_tab_and_blake3_prefix() {
    let line = encode(&Record::Done { item: 3 });
    let (json, hex) = line
        .trim_end_matches('\n')
        .split_once('\t')
        .unwrap_or_default();
    assert_eq!(json, r#"{"t":"Done","item":3}"#);
    assert_eq!(hex, hex_of(json));
}

/// The checksum the format asks for, computed apart from the codec.
pub(super) fn hex_of(json: &str) -> String {
    let hash = blake3::hash(json.as_bytes());
    hash.as_bytes()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn codec_writes_ascii_only_and_keeps_unicode() {
    let record = Record::Intent {
        item: 0,
        target: "/home/admin/Copiază ✓ 😀\t\"q\".txt".into(),
        mode: ItemMode::Overwrite,
        expect_size: None,
        expect_mtime_ms: None,
        restore_mtime_ms: None,
    };
    let line = encode(&record);
    assert!(
        line.is_ascii(),
        "a torn tail must never split a UTF-8 char: {line:?}"
    );
    assert_eq!(line.matches('\t').count(), 1, "a tab in a name is escaped");
    assert_eq!(decode(line.as_bytes()), clean(vec![record]));
}

#[test]
fn codec_empty_file_has_no_records() {
    assert_eq!(decode(b""), clean(Vec::new()));
}

#[test]
fn codec_flipped_byte_in_middle_line_is_damaged() {
    let records = every_variant();
    let text = log_of(&records[..3]);
    let first = encode(&records[0]).len();
    let second = encode(&records[1]).len();
    // Every byte of the middle line but its newline: json, tab and checksum.
    for at in first..first + second - 1 {
        let mut bytes = text.clone().into_bytes();
        bytes[at] ^= 0x01;
        let log = decode(&bytes);
        assert!(log.damaged, "a flip at byte {at} must mark the log damaged");
        assert!(
            !log.torn_tail,
            "a complete bad line is damage, not a torn tail"
        );
        assert_eq!(
            log.records,
            records[..1],
            "nothing after the bad line is read"
        );
    }
}

#[test]
fn codec_bad_line_shapes_are_damaged() {
    let good = encode(&Record::Commit);
    let (json, hex) = good
        .trim_end_matches('\n')
        .split_once('\t')
        .unwrap_or_default();
    assert!(
        hex.contains(|c: char| c.is_ascii_alphabetic()),
        "the fixture needs a hex letter"
    );
    let upper = format!("{json}\t{}\n", hex.to_uppercase());
    let bad_lines = [
        "\n".to_owned(),
        format!("{json}\n"),
        upper,
        format!("{json}\t{}\n", &hex[..15]),
        format!("{json}\t{hex}0\n"),
        format!("{{\"t\":\"Nope\"}}\t{}\n", hex_of("{\"t\":\"Nope\"}")),
    ];
    for bad in bad_lines {
        let log = decode(format!("{good}{bad}{good}").as_bytes());
        assert!(log.damaged, "line {bad:?} must be damage");
        assert_eq!(log.records, vec![Record::Commit]);
    }
}

#[test]
fn codec_cut_last_line_is_torn_tail() {
    let records = every_variant();
    let text = log_of(&records[..3]);
    let last = encode(&records[2]).len();
    for cut in [1, last / 2, last - 1] {
        let log = decode(&text.as_bytes()[..text.len() - cut]);
        assert!(
            log.torn_tail,
            "a last line cut by {cut} bytes is a torn tail"
        );
        assert!(!log.damaged, "a torn tail is not damage");
        assert_eq!(
            log.records,
            records[..2],
            "the records before the tail are kept"
        );
    }
}

#[test]
fn codec_complete_record_without_newline_is_not_trusted() {
    let text = format!(
        "{}{}",
        encode(&Record::Done { item: 0 }),
        encode(&Record::Commit)
    );
    let log = decode(text.trim_end_matches('\n').as_bytes());
    assert!(log.torn_tail, "a line is durable only with its newline");
    assert_eq!(log.records, vec![Record::Done { item: 0 }]);
}

/// Any text: every char, plus NUL, newlines, tab, U+2028 and a BOM often. `any::<String>()`
/// is the regex `\PC*`, which never yields a control or format char.
fn any_text() -> impl Strategy<Value = String> {
    let hostile = prop_oneof![
        Just('\0'),
        Just('\n'),
        Just('\r'),
        Just('\t'),
        Just('\u{2028}'),
        Just('\u{feff}'),
        Just('"'),
        Just('\\'),
    ];
    let one = prop_oneof![any::<char>(), hostile];
    proptest::collection::vec(one, 0..24).prop_map(|chars| chars.into_iter().collect())
}

fn any_stamp() -> impl Strategy<Value = FileStamp> {
    (any::<u64>(), any::<Option<i64>>(), any::<[u8; 32]>()).prop_map(|(size, modified_ms, hash)| {
        FileStamp {
            size,
            modified_ms,
            hash,
        }
    })
}

fn any_record() -> impl Strategy<Value = Record> {
    prop_oneof![
        (any_text(), any::<u64>(), any::<Option<u64>>(), any_text()).prop_map(
            |(label, created_ms, reverts, owner)| Record::Begin {
                job: JobId::new(created_ms, 1, 2),
                kind: JobKind::CopyMany,
                label,
                created_ms,
                reverts: reverts.map(|ms| JobId::new(ms, 3, 4)),
                owner,
            }
        ),
        (
            any::<u32>(),
            any_text(),
            any::<Option<u64>>(),
            any::<Option<i64>>()
        )
            .prop_map(|(item, target, expect_size, mtime)| Record::Intent {
                item,
                target,
                mode: ItemMode::Overwrite,
                expect_size,
                expect_mtime_ms: mtime,
                restore_mtime_ms: mtime,
            }),
        (any::<u32>(), any_stamp()).prop_map(|(item, new)| Record::Staged { item, new }),
        (any::<u32>(), any_stamp()).prop_map(|(item, old)| Record::Backed { item, old }),
        any::<u32>().prop_map(|item| Record::Done { item }),
        Just(Record::Commit),
        any_text().prop_map(|reason| Record::Aborted { reason }),
    ]
}

proptest! {
    /// A crash can cut the log at any byte: the result is the complete lines before the cut.
    #[test]
    fn codec_truncation_never_yields_a_record_past_the_cut(
        records in proptest::collection::vec(any_record(), 0..8),
        cut_seed in any::<usize>(),
    ) {
        let text = log_of(&records);
        prop_assert!(text.is_ascii(), "the log is ASCII, so a cut never splits a char");
        let cut = cut_seed % (text.len() + 1);
        let kept = &text.as_bytes()[..cut];
        let whole_lines = kept.iter().filter(|&&b| b == b'\n').count();
        let good_len = kept.iter().rposition(|&b| b == b'\n').map_or(0, |at| at + 1);
        let log = decode(kept);
        prop_assert!(!log.damaged, "a cut is never damage");
        prop_assert_eq!(log.torn_tail, !kept.is_empty() && !kept.ends_with(b"\n"));
        prop_assert_eq!(&log.records[..], &records[..whole_lines]);
        prop_assert_eq!(log.good_len, good_len);
    }

    /// Any text in any field, NUL, newlines, U+2028 and a BOM included, round-trips.
    #[test]
    fn codec_any_record_round_trips(record in any_record()) {
        let line = encode(&record);
        prop_assert_eq!(line.matches('\n').count(), 1, "one record is one line");
        prop_assert_eq!(decode(line.as_bytes()), clean(vec![record]));
    }
}
