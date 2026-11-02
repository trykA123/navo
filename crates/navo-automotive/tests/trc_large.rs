mod support;

use std::time::Instant;

use support::{entries_of, frame, trace_of};

#[test]
#[ignore = "release benchmark: run explicitly with --release --ignored --nocapture"]
#[allow(
    clippy::print_stderr,
    reason = "explicit benchmark reports its measurement"
)]
fn fifty_megabyte_trc_indexes_and_decodes_a_window() {
    let mut text = String::from(";$FILEVERSION=2.1\n;$COLUMNS=N,O,T,B,I,d,L,D\n");
    let row = "1 1.234 DT 1 0010 Rx 8 00 01 02 03 04 05 06 07\n";
    let count = 50_000_000usize.div_ceil(row.len());
    for _ in 0..count {
        text.push_str(row);
    }
    let started = Instant::now();
    let trace = trace_of(&text);
    let took = started.elapsed();
    eprintln!(
        "TRC: {} bytes, {} rows, {:.3} s",
        text.len(),
        trace.rows(),
        took.as_secs_f64()
    );
    assert!(text.len() >= 50_000_000);
    assert_eq!(trace.rows(), count as u64);
    assert_eq!(trace.malformed, 0);
    assert_eq!(trace.stats[0].count, count as u64);
    let entries = entries_of(&text, &trace, &[0, count as u64 / 2, count as u64 - 1]);
    assert_eq!(entries.len(), 3);
    assert_eq!(frame(&entries[2]).data, (0..8).collect::<Vec<u8>>());
}
