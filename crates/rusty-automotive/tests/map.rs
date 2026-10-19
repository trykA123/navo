use std::{io::BufReader, sync::atomic::AtomicBool};

use proptest::prelude::*;
use rusty_automotive::map::{MapFlavor, MapModel, MapProblem, index_map, sniff_map};

const FIXTURE: &str = "TASKING VX-toolset for TriCore linker map file
* Tool and Invocation
invented invocation
* Processed Files
| File | From archive | Symbol causing the extraction |
| engine.o | invented.a | turn |
* Link Result
| [in] | | | [out] | | |
| File | Section | Size | Offset | Section | Size |
| engine.o | .text.turn (1) | 0x10 | 0x4 | .text (8) | 0x20 |
* Cross References
ignored
* Call Graph
ignored
* Locate Result
* Sections
+ Space chip:core:linear (MAU = 8bit)
+-------------------------+
| Group | Chip | Section | Space addr | Chip addr | Size (MAU) | Alignment |
|=======|======|=========|============|===========|============|===========|
| code | chip:flash | .text (8) | 0x80000000 | 0x0 | 0x20 | 0x4 |
* Locate Rules
ignored
* Memory Usage
| Memory | Code | Data | Reserved | Free | Total |
| chip:flash | 32 | 16 | 16 | 192 | 256 |
* Symbols (sorted on name)
| Name | Address | Space |
| turn | 0x80000004 | chip:core:linear |
* Symbols (sorted on address)
| Space | Address | Name |
| chip:core:linear | 0x80000004 | turn |
* Removed Sections
| Section | File | Library | Symbol | Reason |
| .text.old (7) | old.o | invented.a | old_turn | unreferenced |
* Future Extension
| Name | Address |
| fake_symbol | 0x10 |
";

#[allow(clippy::unwrap_used, reason = "synthetic test fixtures must parse")]
fn parse(text: &[u8]) -> MapModel {
    index_map(text, &AtomicBool::new(false), &mut |_| {}).unwrap()
}

#[test]
fn tasking_tables_feed_the_common_model() {
    let model = parse(FIXTURE.as_bytes());
    assert_eq!(model.flavor, MapFlavor::Tasking);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    let region = &model.regions[0];
    assert_eq!(region.name, "chip:flash");
    assert_eq!(region.origin, None);
    assert_eq!((region.length, region.used, region.free), (256, 64, 192));
    let section = &model.sections[0];
    assert_eq!(
        (&*section.name, section.address, section.size),
        (".text (8)", 0x80000000, 32)
    );
    assert_eq!(section.region.as_deref(), Some("chip:flash"));
    let part = &model.contributions[0];
    assert_eq!(
        (&*part.object_file, &*part.section, part.size),
        ("engine.o", ".text.turn (1)", 16)
    );
    assert_eq!(part.address, Some(0x80000004));
    assert_eq!(model.symbols.len(), 1);
    let symbol = &model.symbols[0];
    assert_eq!(
        (&*symbol.name, symbol.address, symbol.size, &symbol.section),
        ("turn", 0x80000004, None, &None)
    );
    assert_eq!(model.discarded[0].object_file.as_deref(), Some("old.o"));
    assert_eq!(model.discarded[0].name, ".text.old (7)");
    assert_eq!(model.skipped_sections, 6);
    assert_eq!(model.lines, FIXTURE.lines().count() as u64);
}

#[test]
fn sniffing_requires_tasking_evidence() {
    assert_eq!(sniff_map(FIXTURE.as_bytes()), Some(MapFlavor::Tasking));
    assert_eq!(
        sniff_map(b"\xef\xbb\xbfTriCore VX-toolset object linker - mapfile\r\n"),
        Some(MapFlavor::Tasking)
    );
    for text in [
        b"random text".as_slice(),
        b"Memory Configuration\nLinker script and memory map",
        b"",
        b"| Name | Address |",
    ] {
        assert_eq!(sniff_map(text), None);
    }
    assert!(
        index_map(
            b"random text".as_slice(),
            &AtomicBool::new(false),
            &mut |_| {}
        )
        .is_err()
    );
}

#[test]
fn crlf_and_small_reader_chunks_preserve_tables_and_progress() {
    let text = FIXTURE.replace('\n', "\r\n");
    let mut progress = Vec::new();
    let model = index_map(
        BufReader::with_capacity(3, text.as_bytes()),
        &AtomicBool::new(false),
        &mut |n| progress.push(n),
    )
    .unwrap();
    assert_eq!(model, parse(FIXTURE.as_bytes()));
    assert_eq!(progress.last().copied(), Some(text.len() as u64));
    assert!(progress.windows(2).all(|pair| pair[0] <= pair[1]));
}

#[test]
fn malformed_rows_are_reported_and_later_rows_survive() {
    let text = "* Symbols (sorted on name)\n| Name | Address | Size | Section |\n| bad | nope | 1 | .x |\n| short |\n| huge | 0xfffffffffffffffff | 1 | .x |\n| broken_size | 0x10 | nope | .x |\n| valid | 0x20 | 4 | .x |\n";
    let model = parse(text.as_bytes());
    assert_eq!(model.symbols.len(), 1);
    assert_eq!(model.symbols[0].size, Some(4));
    assert_eq!(model.symbols[0].section.as_deref(), Some(".x"));
    assert_eq!(
        model
            .problems
            .iter()
            .map(|p| match p {
                MapProblem::Malformed { line, .. } => *line,
                _ => 0,
            })
            .collect::<Vec<_>>(),
        vec![3, 4, 5, 6]
    );
}

#[test]
fn oversized_lines_are_skipped_without_losing_line_numbers() {
    let text = format!(
        "* Symbols (sorted on name)\n| Name | Address |\n{}\n| after | 0x20 |",
        "x".repeat(3_000_000)
    );
    let model = parse(text.as_bytes());
    assert_eq!(model.lines, 4);
    assert_eq!(model.symbols[0].name, "after");
    assert!(matches!(
        model.problems.as_slice(),
        [MapProblem::Malformed { line: 3, .. }]
    ));
}

#[test]
fn symbols_keep_address_spaces_and_empty_optional_cells() {
    let model = parse(b"* Symbols (sorted on name)\n| Symbol | Address | Space | Section |\n| equal | 0x10 | chip:one ||\n| equal | 0x10 | chip:two ||\n| equal | 0x10 | chip:one ||\n");
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(model.symbols.len(), 2);
    assert!(model.symbols.iter().all(|symbol| symbol.section.is_none()));
}

#[test]
fn link_rows_keep_file_context_and_leave_missing_or_ambiguous_addresses_unknown() {
    let model = parse(b"* Tool and Invocation\n* Link Result\n| [in] File | [in] Section | [in] Size | [out] Offset | [out] Section | [out] Size |\n| alpha.o | .one (1) | 0x4 | 0x4 | .output (4) | 0x10 |\n|| .two (2) | 0x4 | 0x8 | .output (4) | 0x10 |\n| beta.o | .three (3) | 4 | 0 | .missing (5) | 16 |\n* Locate Result\n| Section | Space addr | Size (MAU) |\n| .output (4) | 0x100 | 0x10 |\n| .output (4) | 0x200 | 0x10 |\n");
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(model.contributions.len(), 3);
    assert_eq!(model.contributions[1].object_file, "alpha.o");
    assert!(
        model
            .contributions
            .iter()
            .all(|part| part.address.is_none())
    );
}

#[test]
fn malformed_rows_in_each_supported_table_are_reported() {
    let model = parse(b"* Tool and Invocation\n* Link Result\n| File | Section | Size | Offset | Section | Size |\n| alpha.o | .one | 4 | 15 | .out | 16 |\n* Locate Result\n| Section | Space addr | Size (MAU) |\n| .bad | 1 | invalid |\n* Removed Sections\n| Section | File | Size | Reason |\n| .gone | alpha.o | invalid | unreferenced |\n* Symbols (sorted on name)\n| Name | Address |\nmissing pipe separators\n| recovered | 0x10 |\n");
    assert_eq!(model.problems.len(), 4);
    assert_eq!(model.symbols[0].name, "recovered");
    assert!(
        model.sections.is_empty() && model.contributions.is_empty() && model.discarded.is_empty()
    );
}

proptest! {
    #[test]
    fn arbitrary_input_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..20_000)) {
        let _ = sniff_map(&bytes);
        let _ = index_map(bytes.as_slice(), &AtomicBool::new(false), &mut |_| {});
        let mut with_header = b"* Symbols (sorted on name)\n| Name | Address |\n".to_vec();
        with_header.extend(bytes);
        let _ = parse(&with_header);
    }
}
