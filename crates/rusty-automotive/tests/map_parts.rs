use std::fmt::Write as _;
use std::sync::atomic::AtomicBool;

use rusty_automotive::map::{MapFlavor, MapModel, index_map, sniff_map};

#[allow(clippy::unwrap_used, reason = "synthetic test fixtures must parse")]
fn parse(body: &str) -> MapModel {
    let text = format!("TASKING VX\n*** Link Result ***\n{body}");
    index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {}).unwrap()
}

#[test]
fn map_fifty_call_graph_lines_are_not_unread_parts() {
    let model = parse(&"* invented_call [1,2]\n".repeat(50));
    assert!(model.skipped_parts.is_empty(), "{:?}", model.skipped_parts);
}

#[test]
fn map_three_titled_parts_keep_their_names_in_file_order() {
    let model = parse("*** Invented Alpha ***\n* Invented Beta\n======\n=== Invented Gamma ===\n");
    assert_eq!(
        model.skipped_parts,
        ["Invented Alpha", "Invented Beta", "Invented Gamma"]
    );
}

#[test]
fn map_symbols_repeat_the_previous_space_cell() {
    let model = parse(
        "* Symbols (sorted on name)\n| Name | Address | Space |\n| invented_first | 0x10 | invented:space |\n| invented_second | 0x20 | |\n| invented_third | 0x30 | |\n",
    );
    assert_eq!(model.symbols.len(), 3);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert!(
        model
            .symbols
            .iter()
            .all(|symbol| { symbol.address_space.as_deref() == Some("invented:space") })
    );
}

#[test]
fn map_symbols_without_a_size_column_have_no_sizes() {
    let model = parse("* Symbols\n| Name | Address |\n| invented_symbol | 0x10 |\n");
    assert_eq!(model.symbols.len(), 1);
    assert!(!model.has_symbol_sizes());
}

#[test]
fn map_symbol_sizes_include_zero_and_skip_empty_cells() {
    for (sizes, expected) in [
        ("| |", false),
        ("| - |", false),
        ("| 0 |", true),
        ("| 4 |", true),
    ] {
        let model = parse(&format!(
            "* Symbols\n| Name | Address | Size |\n| invented_first | 0x10 | |\n| invented_second | 0x20 {sizes}\n"
        ));
        assert_eq!(model.symbols.len(), 2);
        assert_eq!(model.has_symbol_sizes(), expected);
    }
}

#[test]
fn map_unread_parts_are_deduplicated_case_insensitively_and_capped() {
    let mut body = "*** Invented First ***\n=== invented first ===\n".to_owned();
    for index in 0..40 {
        writeln!(body, "*** Invented Part {index} ***").unwrap();
    }
    let model = parse(&body);
    assert_eq!(model.skipped_parts.len(), 32);
    assert_eq!(model.skipped_parts[0], "Invented First");
    assert_eq!(model.skipped_parts[1], "Invented Part 0");
    assert_eq!(model.skipped_parts[31], "Invented Part 30");
}

#[test]
fn map_only_supported_banner_shapes_and_known_bare_titles_are_headers() {
    for line in [
        "* invented text",
        "** Invented Text **",
        "== Invented Text ==",
        "*** Invented Text ==",
        "* Symbols invented text",
        "* Memory Usage invented text",
        "* invented_call [1,2]",
        "* invented_call [1,2]\n=====",
    ] {
        let model = parse(line);
        assert!(model.skipped_parts.is_empty(), "{line}");
        assert_eq!(
            sniff_map(format!("TASKING VX\n{line}").as_bytes()),
            None,
            "{line}"
        );
    }
    for line in [
        "* PROCESSED FILES",
        "*** Invented Text ****",
        "==== Invented Text ===",
    ] {
        assert_eq!(parse(line).skipped_parts.len(), 1, "{line}");
    }
}

#[test]
fn map_underlined_headers_need_the_immediately_following_line() {
    let model = parse("* Invented First\n\n====\n* Invented Second\n====\n");
    assert_eq!(model.skipped_parts, ["Invented Second"]);
}

#[test]
fn map_sniffing_and_indexing_recognize_underlined_parts() {
    let text = "TASKING VX\n* Symbols (invented ordering)\n=====\n| Name | Address |\n| invented_symbol | 0x10 |\n";
    assert_eq!(sniff_map(text.as_bytes()), Some(MapFlavor::Tasking));
    let model = index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {}).unwrap();
    assert_eq!(model.symbols.len(), 1);
    assert!(model.problems.is_empty(), "{:?}", model.problems);
}

#[test]
fn map_symbol_spaces_start_at_the_heading_and_reset_between_tables() {
    let model = parse(
        "* Symbols\n+ Space invented:heading (MAU = 8bit)\n| Name | Address | Space |\n| invented_first | 0x10 | |\n| invented_second | 0x20 | invented:cell |\n| invented_third | 0x30 | |\n| Name | Address | Space |\n| invented_fourth | 0x40 | |\n* SYMBOLS\n| Name | Address | Space |\n| invented_fifth | 0x50 | |\n",
    );
    assert!(model.problems.is_empty(), "{:?}", model.problems);
    assert_eq!(
        [
            "invented_first",
            "invented_second",
            "invented_third",
            "invented_fourth",
            "invented_fifth"
        ]
        .map(|name| model
            .symbols
            .iter()
            .find(|symbol| symbol.name == name)
            .unwrap()
            .address_space
            .as_deref()),
        [
            Some("invented:heading"),
            Some("invented:cell"),
            Some("invented:cell"),
            Some("invented:heading"),
            None
        ]
    );
}
