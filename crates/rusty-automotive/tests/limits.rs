mod support;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use rusty_automotive::{Format, IndexError, MAX_PIECES, Problem, Watch, index};
use support::{INTEL_EXAMPLE, intel_hex, parse};

fn biggest_read(text: &str, window: std::ops::Range<u64>) -> (usize, usize) {
    let image = parse(Format::IntelHex, text);
    let bytes = text.as_bytes();
    let (mut biggest, mut calls) = (0, 0);
    let mut read = |offset: u64, len: usize| {
        biggest = biggest.max(len);
        calls += 1;
        let start = offset as usize;
        Ok(bytes[start..(start + len).min(bytes.len())].to_vec())
    };
    assert!(image.read(window, &mut read).is_ok());
    (biggest, calls)
}

#[test]
fn a_line_without_an_end_is_cut_and_reported_as_too_long() {
    let text = format!(
        ":0400000312345678E5\n{}\n:00000001FF\n",
        "A".repeat(3_000_000)
    );
    let image = parse(Format::IntelHex, &text);
    assert_eq!(
        image.problems,
        vec![Problem::Malformed {
            line: 2,
            reason: "the line is too long"
        }]
    );
    assert!(image.has_end);
    assert_eq!(image.lines, 3);
}

#[test]
fn cancel_is_seen_inside_one_endless_line() {
    let text = format!(":00000001FF\n{}", "A".repeat(50_000_000));
    let cancel = AtomicBool::new(true);
    let calls = AtomicUsize::new(0);
    let mut progress = |_| {
        calls.fetch_add(1, Ordering::Relaxed);
    };
    let watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    let result = index(Format::IntelHex, text.as_bytes(), watch);
    assert!(matches!(result, Err(IndexError::Cancelled)));
}

#[test]
fn a_lone_carriage_return_ends_a_line() {
    let text = INTEL_EXAMPLE.replace("\r\n", "\r");
    let image = parse(Format::IntelHex, &text);
    assert!(image.problems.is_empty(), "{:?}", image.problems);
    assert_eq!(image.ranges, vec![0x100..0x140]);
    assert_eq!(image.lines, 5);
    assert_eq!(
        rusty_automotive::sniff(text.as_bytes()),
        Some(Format::IntelHex)
    );
    let bytes = support::read_all(&text, &image, 0x13F..0x140);
    assert_eq!(bytes, vec![Some(0x21)]);
}

#[test]
fn malformed_lines_inside_a_block_keep_window_reads_small() {
    let first = intel_hex(&[(0x0, vec![1; 16])], false);
    let second = intel_hex(&[(0x10, vec![2; 16])], true);
    let junk = "not a record at all, just a long line of text in the middle\n".repeat(10_000);
    let text = format!(
        "{first}{junk}{}",
        second.replacen(":020000040000FA\n", "", 1)
    );
    let (biggest, _) = biggest_read(&text, 0x10..0x20);
    assert!(biggest < 64 * 1024, "{biggest}");
}

#[test]
fn blank_lines_inside_a_block_keep_window_reads_small() {
    let first = intel_hex(&[(0x0, vec![1; 16])], false);
    let second = intel_hex(&[(0x10, vec![2; 16])], true);
    let text = format!(
        "{first}{}{}",
        "\n".repeat(200_000),
        second.replacen(":020000040000FA\n", "", 1)
    );
    let image = parse(Format::IntelHex, &text);
    assert_eq!(image.ranges, vec![0x0..0x20]);
    let (biggest, _) = biggest_read(&text, 0x10..0x20);
    assert!(biggest < 128 * 1024, "{biggest}");
}

#[test]
fn records_on_the_same_address_bound_the_reads_of_one_window() {
    let one = ":0100000001FE\n";
    let text = format!("{}:00000001FF\n", one.repeat(5_000));
    let (_, calls) = biggest_read(&text, 0x0..0x10);
    assert!(calls <= 64, "{calls}");
}

#[test]
fn a_too_fragmented_image_stops_with_a_problem() {
    let blocks: Vec<(u32, Vec<u8>)> = (0..MAX_PIECES as u32 + 10)
        .map(|at| (at * 2, vec![7]))
        .collect();
    let image = parse(Format::IntelHex, &intel_hex(&blocks, true));
    assert!(image.ranges.len() <= MAX_PIECES);
    assert!(
        image
            .problems
            .iter()
            .any(|problem| matches!(problem, Problem::TooFragmented { .. })),
        "{:?}",
        image.problems.last()
    );
}

#[test]
fn address_records_need_their_exact_length() {
    for record in [
        ":0400000400000800F0\n",
        ":020000050800F1\n",
        ":020000030800F3\n",
        ":0400000200000800F2\n",
    ] {
        let image = parse(Format::IntelHex, &format!("{record}:00000001FF\n"));
        assert!(
            matches!(
                image.problems.as_slice(),
                [Problem::Malformed { line: 1, .. }]
            ),
            "{record}: {:?}",
            image.problems
        );
    }
}
