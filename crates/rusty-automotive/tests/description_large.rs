#![allow(clippy::unwrap_used, reason = "benchmark assertions")]

use rusty_automotive::{Watch, index_a2l, index_dbc};
use std::{fmt::Write, sync::atomic::AtomicBool, time::Instant};

fn peak_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")?
            .split_whitespace()
            .next()?
            .parse()
            .ok()
    })
}

#[test]
#[ignore = "release benchmark: run explicitly with --release --ignored --exact --nocapture"]
#[allow(
    clippy::print_stderr,
    reason = "explicit benchmark reports its measurements"
)]
fn fifty_megabyte_a2l_maps_description_rows() {
    let mut text = String::from(
        "ASAP2_VERSION 1 71\n/begin PROJECT P \"\"\n/begin MODULE M \"\"\n/begin COMPU_METHOD Conv \"\" IDENTICAL \"%6.2\" \"unit\" /end COMPU_METHOD\n",
    );
    let description = "invented description ".repeat(20);
    let mut rows = 0;
    while text.len() < 50_000_000 {
        writeln!(text, "/begin MEASUREMENT Value{rows} \"{description}\" UWORD Conv 1 0 0 65535 ECU_ADDRESS 4096 /end MEASUREMENT").unwrap();
        rows += 1;
    }
    text.push_str("/end MODULE\n/end PROJECT\n");
    let started = Instant::now();
    let model = index_a2l(
        text.as_bytes(),
        Watch {
            cancel: &AtomicBool::new(false),
            progress: &mut |_| {},
        },
    )
    .unwrap();
    let elapsed = started.elapsed();
    let peak = peak_rss_kib();
    eprintln!(
        "A2L: {} bytes, {} measurements, {:.3} s, peak RSS {:?} KiB",
        text.len(),
        model.measurements.len(),
        elapsed.as_secs_f64(),
        peak
    );
    assert_eq!(model.measurements.len(), rows);
    assert_eq!(model.report.omitted, 0);
    assert_eq!(model.report.problem_count, 0);
    assert_eq!(model.measurements[rows - 1].unit.as_deref(), Some("unit"));
    if let Some(peak) = peak {
        assert!(peak < 200_000_000 / 1024, "peak RSS: {peak} KiB");
    }
}

#[test]
#[ignore = "release benchmark: run explicitly with --release --ignored --exact --nocapture"]
#[allow(
    clippy::print_stderr,
    reason = "explicit benchmark reports its measurements"
)]
fn five_megabyte_dbc_maps_messages_and_signals() {
    let mut text = String::from("VERSION \"invented\"\nBU_: Sender Receiver\n");
    let mut messages = 0;
    while text.len() < 5_000_000 {
        let id = (1u32 << 31) + messages;
        writeln!(text, "BO_ {id} Frame{messages}: 8 Sender\n SG_ Mode M : 0|8@1+ (1,0) [0|255] \"\" Receiver\n SG_ Value m3 : 15|16@0- (0.5,-10) [-100|100] \"deg\" Receiver\nCM_ BO_ {id} \"Invented message\";\nBA_ \"GenMsgCycleTime\" BO_ {id} 10;\nVAL_ {id} Mode 0 \"Off\" 3 \"On\";").unwrap();
        messages += 1;
    }
    let started = Instant::now();
    let model = index_dbc(
        text.as_bytes(),
        Watch {
            cancel: &AtomicBool::new(false),
            progress: &mut |_| {},
        },
    )
    .unwrap();
    let elapsed = started.elapsed();
    let peak = peak_rss_kib();
    eprintln!(
        "DBC: {} bytes, {} messages, {:.3} s, peak RSS {:?} KiB",
        text.len(),
        model.messages.len(),
        elapsed.as_secs_f64(),
        peak
    );
    assert_eq!(model.messages.len(), messages as usize);
    assert_eq!(model.report.omitted, 0);
    assert_eq!(model.report.problem_count, 0);
    assert_eq!(model.messages.last().unwrap().signals.len(), 2);
    assert_eq!(model.messages.last().unwrap().cycle_time_ms, Some(10.0));
    if let Some(peak) = peak {
        assert!(peak < 200_000_000 / 1024, "peak RSS: {peak} KiB");
    }
}
