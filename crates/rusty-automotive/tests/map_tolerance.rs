use std::sync::atomic::AtomicBool;

use rusty_automotive::map::{MapFlavor, MapModel, index_map, sniff_map};

#[allow(clippy::unwrap_used, reason = "synthetic test fixtures must parse")]
fn parse(body: &str) -> MapModel {
    let text = format!("TASKING VX-toolset linker map\n{body}");
    index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {}).unwrap()
}

const LINK_COLUMNS: &str = "| [in] File | [in] Section | [in] Size (MAU) | [out] Offset (hex) | [out] Section | [out] Size (MAU) |\n";

#[test]
fn each_part_banner_style_is_recognized() {
    for header in [
        "* Link Result",
        "*********  Link Result  *********",
        "===== Link Result =====",
    ] {
        let body = format!("{header}\n{LINK_COLUMNS}| orbit.o | .orbit | 4 | 0 | .code | 8 |\n");
        let model = parse(&body);
        assert_eq!(
            model.contributions.len(),
            1,
            "{header}: {:?}",
            model.problems
        );
        assert!(model.problems.is_empty());
        assert_eq!(
            sniff_map(format!("ltc linker\n{header}").as_bytes()),
            Some(MapFlavor::Tasking)
        );
    }
}

#[test]
fn output_size_units_are_validated_separately_from_input_size() {
    let model = parse(&format!(
        "* Link Result\n{LINK_COLUMNS}| orbit.o | .orbit | 4 | 6 | .code | 8 |\n"
    ));
    assert!(model.contributions.is_empty());
    assert_eq!(model.problems.len(), 1);
}

#[test]
fn numeric_column_units_control_the_radix() {
    let model = parse(
        "* Locate Result\n| Section | Space addr (hex) | Sect. Size (dec) |\n| .orbit | 8abc | 16 |\n",
    );
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(
        (model.sections[0].address, model.sections[0].size),
        (0x8abc, 16)
    );
}

#[test]
fn symbols_accept_each_address_column() {
    for column in ["Address", "Value", "Space addr", "Chip addr"] {
        let model = parse(&format!(
            "* Symbols (sorted on name)\n| Name | {column} |\n| orbit | 0x40 |\n"
        ));
        assert_eq!(model.symbols.len(), 1, "{column}: {:?}", model.problems);
        assert_eq!(model.symbols[0].address, 64);
    }
}

#[test]
fn lossy_paths_are_retained_and_reported_once() {
    let mut text = format!("TASKING VX\n* Link Result\n{LINK_COLUMNS}").into_bytes();
    text.extend_from_slice(b"| orbit\xe9.o | .orbit | 4 | 0 | .code | 16 |\n| lunar\xe9.o | .lunar | 4 | 4 | .code | 16 |\n");
    let model = index_map(text.as_slice(), &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!(model.contributions.len(), 2);
    assert_eq!(model.contributions[0].object_file, "orbit\u{fffd}.o");
    assert_eq!(model.problems.len(), 1);
}

#[test]
fn dropped_link_rows_clear_the_carried_file() {
    for broken in [
        "| short |".to_owned(),
        "x".repeat(100_000),
        "missing pipes".to_owned(),
        "| lunar.o | .lunar | nope | 0 | .code | 16 |".to_owned(),
    ] {
        let model = parse(&format!(
            "* Link Result\n| File | Section | Size | Offset | Section | Size |\n| orbit.o | .orbit | 4 | 0 | .code | 16 |\n{broken}\n|| .stray | 4 | 4 | .code | 16 |\n| comet.o | .comet | 4 | 8 | .code | 16 |\n"
        ));
        assert_eq!(model.contributions.len(), 2, "{:?}", model.problems);
        assert_eq!(model.contributions[1].object_file, "comet.o");
        assert_eq!(model.problems.len(), 2);
    }
}

#[test]
fn second_symbol_ordering_is_not_retained() {
    let model = parse(
        "* Symbols (sorted on name)\n| Name | Address |\n| orbit | 0x10 |\n* Symbols (sorted on address)\n| Name | Address |\n| duplicate_order_only | 0x20 |\n",
    );
    assert_eq!(model.symbols.len(), 1);
    assert_eq!(model.symbols[0].name, "orbit");
    assert!(model.problems.is_empty());
}

#[test]
fn memory_totals_are_skipped_and_inconsistent_regions_are_retained() {
    let model = parse(
        "* Memory Usage\n| Memory | Code | Data | Reserved | Free | Total |\n| orbit_ram | 1 | 2 | 3 | 8 | 16 |\n| TOTAL | - | - | - | - | - |\n| Total | 1 | 2 | 3 | 8 | 16 |\n",
    );
    assert_eq!(model.regions.len(), 1);
    assert_eq!((model.regions[0].used, model.regions[0].free), (8, 8));
    assert_eq!(model.problems.len(), 1);
}

#[test]
fn sniffing_requires_a_tool_marker_and_part_within_two_hundred_lines() {
    for text in [
        "multitasking linker bitmap\n* Link Result\n",
        "TASKING VX linker map",
        "* Link Result",
        "ltcish\n* Link Result",
        "ltc\n* Future Extension",
    ] {
        assert_eq!(sniff_map(text.as_bytes()), None, "{text}");
        assert!(index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {}).is_err());
    }
    for marker in ["TASKING VX", "VX-toolset", "ltc"] {
        let text = format!("{marker}\n{}===== Link Result =====", "\n".repeat(198));
        assert_eq!(sniff_map(text.as_bytes()), Some(MapFlavor::Tasking));
        let late = format!("{marker}\n{}* Link Result", "\n".repeat(199));
        assert_eq!(sniff_map(late.as_bytes()), None);
        assert!(index_map(late.as_bytes(), &AtomicBool::new(false), &mut |_| {}).is_err());
    }
}

#[test]
fn unsupported_mau_is_only_reported_inside_parsed_parts() {
    let model = parse(
        "+ Space orbit:ignored (MAU = 16bit)\n* Tool and Invocation\n+ Space orbit:ignored (MAU = 16bit)\n* Locate Result\n+ Space orbit:wide (MAU = 16bit)\n| Section | Address | Size |\n| .ignored | 0x10 | 4 |\n+ Space orbit:byte (MAU = 8bit)\n| Section | Address | Size |\n| .kept | 0x20 | 4 |\n",
    );
    assert_eq!(model.problems.len(), 1);
    assert_eq!(model.sections.len(), 1);
}

#[test]
fn symbol_caps_do_not_starve_regions_sections_or_contributions() {
    let body = format!(
        "* Symbols (sorted on name)\n| Name | Address |\n{}* Locate Result\n| Section | Address | Size | Chip |\n| .code | 0x100 | 16 | orbit_ram |\n* Memory Usage\n| Memory | Code | Data | Reserved | Free | Total |\n| orbit_ram | 16 | 0 | 0 | 16 | 32 |\n* Link Result\n| File | Section | Size | Offset | Section | Size |\n| orbit.o | .orbit | 4 | 0 | .code | 16 |\n",
        "| orbit | 1 |\n".repeat(rusty_automotive::map::MAX_MAP_ENTRIES + 2)
    );
    let model = parse(&body);
    assert_eq!(model.sections.len(), 1);
    assert_eq!(model.regions.len(), 1);
    assert_eq!(model.contributions.len(), 1);
    assert_eq!(model.problems.len(), 1);
}

#[test]
fn prefixed_output_size_is_found_before_the_output_section() {
    let model = parse(
        "* Link Result\n| [out] Size (MAU) | [in] Size (MAU) | [in] File | [out] Offset | [in] Section | [out] Section |\n| 8 | 4 | orbit.o | 6 | .orbit | .code |\n",
    );
    assert!(model.contributions.is_empty());
    assert_eq!(model.problems.len(), 1);
}

#[test]
fn an_empty_symbol_ordering_does_not_hide_a_later_populated_ordering() {
    let model = parse(
        "* Symbols (sorted on name)\n| Name | Address |\n* Symbols (sorted on address)\n| Name | Address |\n| orbit | 0x10 |\n",
    );
    assert_eq!(model.symbols.len(), 1);
}

#[test]
fn oversized_tool_banners_do_not_bypass_the_line_cap_when_sniffing() {
    let text = format!("TASKING VX {}\n* Link Result\n", " ".repeat(8000));
    assert_eq!(sniff_map(text.as_bytes()), None);
    assert!(index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {}).is_err());
}
