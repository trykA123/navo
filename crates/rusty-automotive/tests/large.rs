mod support;

use std::time::Instant;

use rusty_automotive::Format;
use support::{parse, s_records};

#[test]
fn a_fifty_megabyte_s19_is_indexed_with_its_ranges_and_errors() {
    let block: Vec<u8> = (0..21_000_000u32).map(|at| (at % 253) as u8).collect();
    let blocks = [
        (0x0000_0000, block[..12_000_000].to_vec()),
        (0x0100_0000, block[12_000_000..].to_vec()),
    ];
    let mut text = s_records(&blocks, 0);
    let broken = text.find("\nS3").unwrap() + 1 + 200 * 75;
    let line_end = broken + text[broken..].find('\n').unwrap();
    let fixed = if &text[line_end - 2..line_end] == "00" {
        "01"
    } else {
        "00"
    };
    text.replace_range(line_end - 2..line_end, fixed);
    assert!(text.len() > 50_000_000, "{}", text.len());
    let started = Instant::now();
    let image = parse(Format::SRecord, &text);
    let took = started.elapsed();
    assert_eq!(
        image.ranges,
        vec![0..12_000_000, 0x0100_0000..0x0100_0000 + 9_000_000]
    );
    assert_eq!(image.checksum_errors, 1);
    assert!(took.as_secs() < 10, "{took:?}");
}
