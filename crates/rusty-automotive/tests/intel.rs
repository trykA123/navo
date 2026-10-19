mod support;

use rusty_automotive::{Format, Item, Problem, Start};
use support::{INTEL_EXAMPLE, intel_hex, parse, read_all};

#[test]
fn the_example_file_is_one_range_with_an_end_record() {
    let image = parse(Format::IntelHex, INTEL_EXAMPLE);
    assert_eq!(image.ranges, vec![0x100..0x140]);
    assert!(image.has_end);
    assert!(image.problems.is_empty());
    assert_eq!(image.data_len(), 64);
    assert_eq!(image.lines, 5);
}

#[test]
fn records_are_counted_by_type() {
    let image = parse(Format::IntelHex, INTEL_EXAMPLE);
    assert_eq!(image.counts.get(0), 4);
    assert_eq!(image.counts.get(1), 1);
    assert_eq!(image.counts.get(4), 0);
}

#[test]
fn a_wrong_checksum_names_the_line_and_both_values() {
    let text = INTEL_EXAMPLE.replacen("D2190140", "D2190141", 1);
    let image = parse(Format::IntelHex, &text);
    assert_eq!(
        image.problems,
        vec![Problem::Checksum {
            line: 1,
            stored: 0x41,
            computed: 0x40
        }]
    );
    assert_eq!(image.checksum_errors, 1);
}

#[test]
fn a_record_with_a_wrong_checksum_still_shows_its_data() {
    let text = INTEL_EXAMPLE.replacen("D2190140", "D2190141", 1);
    let image = parse(Format::IntelHex, &text);
    assert_eq!(image.ranges, vec![0x100..0x140]);
}

#[test]
fn a_file_without_an_end_record_says_so() {
    let text = INTEL_EXAMPLE.replace(":00000001FF\r\n", "");
    let image = parse(Format::IntelHex, &text);
    assert!(!image.has_end);
}

#[test]
fn a_line_that_is_not_hex_is_malformed() {
    let text = INTEL_EXAMPLE.replacen(":100110002146", ":10011000ZZ46", 1);
    let image = parse(Format::IntelHex, &text);
    assert!(matches!(
        image.problems.as_slice(),
        [Problem::Malformed { line: 2, .. }]
    ));
    assert_eq!(image.ranges, vec![0x100..0x110, 0x120..0x140]);
}

#[test]
fn a_byte_count_that_does_not_match_the_line_is_malformed() {
    let text = INTEL_EXAMPLE.replacen(":10012000", ":11012000", 1);
    let image = parse(Format::IntelHex, &text);
    assert!(matches!(
        image.problems.as_slice(),
        [Problem::Malformed { line: 3, .. }]
    ));
}

#[test]
fn blank_lines_are_skipped_but_counted() {
    let text = format!("\n{INTEL_EXAMPLE}\n");
    let image = parse(Format::IntelHex, &text);
    assert!(image.problems.is_empty());
    assert_eq!(image.lines, 7);
}

#[test]
fn two_blocks_leave_one_gap() {
    let text = intel_hex(&[(0x1000, vec![1; 32]), (0x1100, vec![2; 16])], true);
    let image = parse(Format::IntelHex, &text);
    assert_eq!(image.ranges, vec![0x1000..0x1020, 0x1100..0x1110]);
    assert_eq!(image.gaps(), vec![0x1020..0x1100]);
}

#[test]
fn items_alternate_ranges_and_gaps_in_address_order() {
    let text = intel_hex(&[(0x10, vec![1; 16]), (0x40, vec![2; 16])], true);
    let image = parse(Format::IntelHex, &text);
    assert_eq!(
        image.items(),
        vec![
            Item::Range(0x10..0x20),
            Item::Gap(0x20..0x40),
            Item::Range(0x40..0x50)
        ]
    );
    assert_eq!(image.item_at(0x30), Some(1));
    assert_eq!(image.item_at(0x4F), Some(2));
    assert_eq!(image.item_at(0x50), None);
}

#[test]
fn extended_linear_addresses_set_the_upper_sixteen_bits() {
    let text = intel_hex(&[(0x0800_0000, vec![7; 20])], true);
    let image = parse(Format::IntelHex, &text);
    assert_eq!(image.ranges, vec![0x0800_0000..0x0800_0014]);
    assert_eq!(image.counts.get(4), 1);
}

#[test]
fn extended_segment_addresses_shift_by_four_bits() {
    let text = ":020000021000EC\n:0400000001020304F2\n:00000001FF\n";
    let image = parse(Format::IntelHex, text);
    assert!(image.problems.is_empty(), "{:?}", image.problems);
    assert_eq!(image.ranges, vec![0x1_0000..0x1_0004]);
}

#[test]
fn start_linear_and_start_segment_records_give_the_start_address() {
    let linear = parse(Format::IntelHex, ":0400000508000131BD\n:00000001FF\n");
    assert_eq!(linear.start, Some(Start::Linear(0x0800_0131)));
    let segment = parse(Format::IntelHex, ":0400000312345678E5\n:00000001FF\n");
    assert_eq!(
        segment.start,
        Some(Start::Segment {
            cs: 0x1234,
            ip: 0x5678
        })
    );
}

#[test]
fn records_that_write_the_same_address_twice_overlap() {
    let text = format!(
        "{}{}",
        intel_hex(&[(0x100, vec![1; 16])], false),
        INTEL_EXAMPLE
    );
    let image = parse(Format::IntelHex, &text);
    assert!(
        image.problems.iter().any(
            |problem| matches!(problem, Problem::Overlap { range, .. } if *range == (0x100..0x110))
        ),
        "{:?}",
        image.problems
    );
    assert_eq!(image.ranges, vec![0x100..0x140]);
}

#[test]
fn records_after_the_end_record_are_reported() {
    let text = format!("{INTEL_EXAMPLE}:0100000001FE\n");
    let image = parse(Format::IntelHex, &text);
    assert_eq!(image.problems, vec![Problem::AfterEnd { line: 6 }]);
    assert_eq!(image.ranges, vec![0x100..0x140]);
}

#[test]
fn an_unknown_record_type_is_malformed() {
    let image = parse(Format::IntelHex, ":0000000AF6\n:00000001FF\n");
    assert!(matches!(
        image.problems.as_slice(),
        [Problem::Malformed { line: 1, .. }]
    ));
}

#[test]
fn a_window_reads_bytes_and_leaves_gaps_empty() {
    let text = intel_hex(&[(0x10, vec![0xAA; 4]), (0x18, vec![0xBB; 2])], true);
    let image = parse(Format::IntelHex, &text);
    let bytes = read_all(&text, &image, 0x12..0x1A);
    assert_eq!(
        bytes,
        vec![
            Some(0xAA),
            Some(0xAA),
            None,
            None,
            None,
            None,
            Some(0xBB),
            Some(0xBB)
        ]
    );
}

#[test]
fn a_window_in_the_middle_of_a_large_block_reads_the_right_bytes() {
    let data: Vec<u8> = (0..40_000u32).map(|at| (at % 251) as u8).collect();
    let text = intel_hex(&[(0x2_0000, data.clone())], true);
    let image = parse(Format::IntelHex, &text);
    let bytes = read_all(&text, &image, 0x2_0000 + 30_000..0x2_0000 + 30_100);
    let expected: Vec<Option<u8>> = data[30_000..30_100].iter().copied().map(Some).collect();
    assert_eq!(bytes, expected);
}
