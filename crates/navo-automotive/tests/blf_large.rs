mod blf_build;

use std::time::Instant;

use blf_build::*;

#[test]
#[ignore = "release benchmark: run explicitly with --release --ignored --nocapture"]
#[allow(
    clippy::print_stderr,
    reason = "explicit benchmark reports its measurement"
)]
fn fifty_megabyte_blf_indexes_and_decodes_a_window() {
    let object = object(1, 1, 100, &classic(1, 0, 16));
    let inner = object.repeat(4096);
    let packed = container(&inner, false);
    let count = 50_000_000usize.div_ceil(packed.len());
    let bytes = file(&vec![packed; count], (count * 4096) as u32);
    let started = Instant::now();
    let trace = trace(&bytes);
    let took = started.elapsed();
    eprintln!(
        "BLF: {} bytes, {} rows, {:.3} s",
        bytes.len(),
        trace.rows(),
        took.as_secs_f64()
    );
    assert!(bytes.len() >= 50_000_000);
    assert_eq!(trace.rows(), (count * 4096) as u64);
    assert_eq!(trace.malformed, 0);
    assert_eq!(trace.stats[0].count, trace.rows());
    let decoded = entries(&bytes, &trace, &[0, trace.rows() / 2, trace.rows() - 1]);
    assert_eq!(decoded.len(), 3);
    assert_eq!(frame(&decoded[2]).data, (0..8).collect::<Vec<u8>>());
}
