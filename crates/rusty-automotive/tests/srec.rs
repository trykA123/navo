mod support;

use std::sync::atomic::{AtomicBool, Ordering};

use rusty_automotive::{Format, IndexError, Problem, Start, Watch, index, sniff};
use support::{INTEL_EXAMPLE, SREC_EXAMPLE, parse, read_all, s_records};

#[test]
fn the_example_file_is_one_range_with_a_start_address() {
    let image = parse(Format::SRecord, SREC_EXAMPLE);
    assert_eq!(image.ranges, vec![0..0x46]);
    assert_eq!(image.start, Some(Start::Linear(0)));
    assert!(image.has_end);
    assert!(image.problems.is_empty(), "{:?}", image.problems);
}

#[test]
fn s_records_are_counted_by_type() {
    let image = parse(Format::SRecord, SREC_EXAMPLE);
    let counts: Vec<u64> = (0..10).map(|kind| image.counts.get(kind)).collect();
    assert_eq!(counts, vec![1, 3, 0, 0, 0, 1, 0, 0, 0, 1]);
}

#[test]
fn thirty_two_bit_addresses_are_read() {
    let text = s_records(&[(0x8000_0000, vec![5; 40])], 0x8000_0004);
    let image = parse(Format::SRecord, &text);
    assert_eq!(image.ranges, vec![0x8000_0000..0x8000_0028]);
    assert_eq!(image.start, Some(Start::Linear(0x8000_0004)));
    assert_eq!(image.counts.get(3), 2);
    assert_eq!(image.counts.get(7), 1);
}

#[test]
fn twenty_four_bit_records_are_read() {
    let image = parse(Format::SRecord, "S206012345AABB2B\nS804000000FB\n");
    assert!(image.problems.is_empty(), "{:?}", image.problems);
    assert_eq!(image.ranges, vec![0x01_2345..0x01_2347]);
}

#[test]
fn a_wrong_s_record_checksum_names_the_line() {
    let text = SREC_EXAMPLE.replacen("0A0042", "0A0043", 1);
    let image = parse(Format::SRecord, &text);
    assert_eq!(
        image.problems,
        vec![Problem::Checksum {
            line: 4,
            stored: 0x43,
            computed: 0x42
        }]
    );
}

#[test]
fn a_file_without_a_termination_record_has_no_end() {
    let text = SREC_EXAMPLE.replace("S9030000FC\n", "");
    let image = parse(Format::SRecord, &text);
    assert!(!image.has_end);
    assert_eq!(image.start, None);
}

#[test]
fn the_reserved_s4_type_is_malformed() {
    let image = parse(Format::SRecord, "S4030000FC\nS9030000FC\n");
    assert!(matches!(
        image.problems.as_slice(),
        [Problem::Malformed { line: 1, .. }]
    ));
}

#[test]
fn a_window_reads_s_record_bytes() {
    let image = parse(Format::SRecord, SREC_EXAMPLE);
    let bytes = read_all(SREC_EXAMPLE, &image, 0x38..0x3D);
    let expected: Vec<Option<u8>> = b"Hello".iter().copied().map(Some).collect();
    assert_eq!(bytes, expected);
}

#[test]
fn sniffing_tells_the_two_formats_apart() {
    assert_eq!(sniff(INTEL_EXAMPLE.as_bytes()), Some(Format::IntelHex));
    assert_eq!(sniff(SREC_EXAMPLE.as_bytes()), Some(Format::SRecord));
}

#[test]
fn sniffing_skips_a_byte_order_mark_and_blank_lines() {
    let text = format!("\u{feff}\n\n{SREC_EXAMPLE}");
    assert_eq!(sniff(text.as_bytes()), Some(Format::SRecord));
}

#[test]
fn sniffing_rejects_text_that_only_looks_close() {
    assert_eq!(sniff(b"Some release notes\n:not a record\n"), None);
    assert_eq!(sniff(b":10 is a label here\n"), None);
    assert_eq!(sniff(b"SELECT * FROM table;\n"), None);
    assert_eq!(sniff(b""), None);
}

#[test]
fn indexing_stops_when_cancelled() {
    let text = s_records(&[(0, vec![1; 32 * 10_000])], 0);
    let cancel = AtomicBool::new(true);
    let mut progress = |_| {};
    let mut watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    let result = index(Format::SRecord, text.as_bytes(), &mut watch);
    assert!(matches!(result, Err(IndexError::Cancelled)));
}

#[test]
fn indexing_reports_progress_in_bytes() {
    let text = s_records(&[(0, vec![1; 32 * 10_000])], 0);
    let cancel = AtomicBool::new(false);
    let mut last = 0;
    let mut progress = |done| last = done;
    let mut watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    index(Format::SRecord, text.as_bytes(), &mut watch).unwrap();
    assert!(last > 0 && last <= text.len() as u64);
    assert!(!cancel.load(Ordering::Relaxed));
}
