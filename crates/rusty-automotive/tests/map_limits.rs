use std::{
    io::{self, BufReader, Read},
    sync::atomic::{AtomicBool, Ordering},
};

use rusty_automotive::map::{MAX_MAP_ENTRIES, MAX_MAP_PROBLEMS, MapError, MapProblem, index_map};

const HEADER: &str = "* Symbols (sorted on name)\n| Name | Address |\n";

#[allow(clippy::unwrap_used, reason = "synthetic test fixtures must parse")]
fn parse(text: &str) -> rusty_automotive::map::MapModel {
    index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {}).unwrap()
}

#[test]
fn cancellation_is_checked_before_reading_and_during_an_endless_line() {
    let cancel = AtomicBool::new(true);
    assert!(matches!(
        index_map(io::empty(), &cancel, &mut |_| {}),
        Err(MapError::Cancelled)
    ));
    cancel.store(false, Ordering::Relaxed);
    let source = HEADER.as_bytes().chain(io::repeat(b'x').take(50_000_000));
    let result = index_map(BufReader::new(source), &cancel, &mut |_| {
        cancel.store(true, Ordering::Relaxed)
    });
    assert!(matches!(result, Err(MapError::Cancelled)));
}

#[test]
fn read_failures_are_propagated() {
    struct Broken;
    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("invented failure"))
        }
    }
    let source = HEADER.as_bytes().chain(Broken);
    assert!(matches!(
        index_map(BufReader::new(source), &AtomicBool::new(false), &mut |_| {}),
        Err(MapError::Read(_))
    ));
}

#[test]
fn entries_are_capped_across_the_model_and_input_is_fully_scanned() {
    let text = format!(
        "{HEADER}{}",
        "| invented | 1 |\n".repeat(MAX_MAP_ENTRIES + 2)
    );
    let mut end = 0;
    let model = index_map(text.as_bytes(), &AtomicBool::new(false), &mut |n| end = n).unwrap();
    assert_eq!(end, text.len() as u64);
    assert_eq!(model.symbols.len(), 1);
    assert!(
        matches!(model.problems.as_slice(), [MapProblem::CapReached { line, limit: "retained entries" }] if *line == MAX_MAP_ENTRIES as u64 + 3)
    );
}

#[test]
fn retained_strings_and_diagnostics_have_independent_caps() {
    let row = format!("| {} | 1 |\n", "x".repeat(4000));
    let model = parse(&format!("{HEADER}{}", row.repeat(8500)));
    assert!(matches!(
        model.problems.as_slice(),
        [MapProblem::CapReached {
            limit: "retained text bytes",
            ..
        }]
    ));
    let model = parse(&format!(
        "{HEADER}{}",
        "| broken | nope |\n".repeat(MAX_MAP_PROBLEMS + 10)
    ));
    assert_eq!(model.problem_total, (MAX_MAP_PROBLEMS + 10) as u64);
    assert_eq!(model.problems.len(), MAX_MAP_PROBLEMS + 1);
    assert!(matches!(
        model.problems.last(),
        Some(MapProblem::CapReached {
            limit: "retained problems",
            ..
        })
    ));
}

#[test]
fn a_four_kibibyte_line_is_accepted_and_one_more_byte_is_rejected() {
    let row = format!("| {} | 1 |", "x".repeat(4088));
    assert_eq!(row.len(), 4096);
    let model = parse(&format!("{HEADER}{row}\r\n{row} \n| last | 2 |\n"));
    assert_eq!(model.symbols.len(), 2);
    assert!(matches!(
        model.problems.as_slice(),
        [MapProblem::Malformed { line: 4, .. }]
    ));
}

#[test]
fn overflowing_ranges_and_inconsistent_memory_totals_are_problems() {
    let text = "* Locate Result\n| Section | Size (MAU) | Space addr |\n| .overflow | 2 | 0xffffffffffffffff |\n* Memory Usage\n| Memory | Code | Data | Reserved | Free | Total |\n| flash | 1 | 2 | 3 | 4 | 5 |\n| flash | 1 | 2 | 3 | 7 | 6 |\n";
    let model = parse(text);
    assert_eq!(model.problems.len(), 3);
    assert!(model.sections.is_empty() && model.regions.is_empty());
}

#[test]
fn non_byte_address_spaces_are_reported_and_skipped() {
    let text = "* Locate Result\n+ Space chip:wide (MAU = 16bit)\n| Section | Size (MAU) | Space addr |\n| .wide | 2 | 0x10 |\n+ Space chip:byte (MAU = 8bit)\n| Section | Size (MAU) | Space addr |\n| .byte | 2 | 0x10 |\n";
    let model = parse(text);
    assert_eq!(model.problems.len(), 1);
    assert_eq!(model.sections.len(), 1);
    assert_eq!(model.sections[0].name, ".byte");
}

#[test]
fn lower_case_headers_and_optional_origin_are_supported() {
    let model = parse(
        "* Tool and Invocation\n* Memory usage in bytes\n| total | free | data | reserved | memory | code | origin |\n| 256 | 192 | 16 | 16 | flash | 32 | 0x8000 |\n",
    );
    assert!(model.problems.is_empty());
    assert_eq!(model.regions[0].origin, Some(0x8000));
}
