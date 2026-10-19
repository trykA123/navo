use std::{
    fmt::Write,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use rusty_automotive::map::index_map;

#[test]
fn fifty_megabytes_with_two_hundred_thousand_symbols_are_indexed() {
    let mut text = String::with_capacity(55_000_000);
    text.push_str("* Symbols (sorted on name)\n| Name | Address | Space |\n");
    for i in 0..200_000u32 {
        writeln!(
            text,
            "| invented_{i:06} {:200} | 0x{:08x} | chip:core:linear |",
            "",
            0x80000000u32 + i * 4
        )
        .unwrap();
    }
    assert!(text.len() >= 50_000_000);
    let started = Instant::now();
    let model = index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {}).unwrap();
    let elapsed = started.elapsed();
    assert!(model.problems.is_empty());
    assert_eq!(model.symbols.len(), 200_000);
    let budget = if cfg!(debug_assertions) {
        Duration::from_secs(30)
    } else {
        Duration::from_secs(2)
    };
    assert!(elapsed < budget, "indexing took {elapsed:?}");
}
