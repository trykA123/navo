use std::fmt::Write as _;

mod support;

use std::time::Instant;

use support::{entries_of, trace_of};

#[test]
fn a_million_line_log_is_indexed_and_any_row_decodes() {
    let mut text =
        String::from("date Mon Oct 19 09:41:07.000 am 2026\nbase hex  timestamps absolute\n");
    for at in 0..1_000_000u64 {
        let id = 0x100 + at % 50;
        writeln!(
            text,
            "{:>4}.{:06} 1  {id:X}             Rx   d 8 {:02X} 01 02 03 04 05 06 07",
            at / 10_000,
            at % 10_000 * 100,
            at % 256
        )
        .unwrap();
    }
    let started = Instant::now();
    let trace = trace_of(&text);
    let took = started.elapsed();
    assert_eq!(trace.rows(), 1_000_000);
    assert_eq!(trace.stats.len(), 50);
    let entries = entries_of(&text, &trace, &[0, 543_210, 999_999]);
    assert_eq!(entries[1].row, 543_210);
    assert_eq!(entries[1].time_ns, Some(54_321_000_000));
    assert!(took.as_secs() < 20, "{took:?}");
}
