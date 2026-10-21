use rusty_automotive::parse_mdf;

use super::text::{rows, spaces_detail, status};
use crate::window_view::viewer::{Kinds, Sniff};

pub(in crate::window_view::viewer) const DEMO: &str = "\
# invented memory definition
space linear 1 8
space data 2 16
chip pflash0 10 8 0x4000 rom
chip dspr0 20 8 0x2000 ram
chip spare 30 8 0x100 ram
map 1 0x80000000 0x2000 10 0
map 1 0x80002000 0x2000 10 0x2000
map 2 0x70000000 0x2000 20 0
";

fn chosen(name: &str, head: &[u8]) -> Option<&'static str> {
    let sniff = Sniff {
        head,
        name,
        len: head.len() as u64,
    };
    Kinds::builtin()
        .choose(&sniff)
        .first()
        .map(|kind| kind.id())
}

#[test]
fn the_kind_is_chosen_for_mdf_text_only() {
    assert_eq!(
        chosen("engine.mdf", DEMO.as_bytes()),
        Some("memory-definition")
    );
    assert_eq!(
        chosen("ENGINE.MDF", DEMO.as_bytes()),
        Some("memory-definition")
    );
    assert_ne!(
        chosen("notes.txt", DEMO.as_bytes()),
        Some("memory-definition")
    );
    assert_ne!(
        chosen("measurement.mdf", b"MDF     4.10\0space local 1 8"),
        Some("memory-definition")
    );
}

#[test]
fn the_chips_table_has_a_row_per_mapping_and_one_for_an_unmapped_chip() {
    let table = rows(&parse_mdf(DEMO));
    let joined: Vec<String> = table.iter().map(|row| row.join(" | ")).collect();
    assert_eq!(
        joined,
        [
            "pflash0 | ROM | 16.0 KB | linear | 0x80000000 | 0x80001FFF | 0x00000000",
            "pflash0 | ROM | 16.0 KB | linear | 0x80002000 | 0x80003FFF | 0x00002000",
            "dspr0 | RAM | 8.0 KB | data | 0x70000000 | 0x70001FFF | 0x00000000",
            "spare | RAM | 256 B |  |  |  | ",
        ]
    );
}

#[test]
fn the_spaces_are_listed_for_the_header() {
    assert_eq!(
        spaces_detail(&parse_mdf(DEMO)),
        "Spaces: linear (id 1, 8-bit), data (id 2, 16-bit)"
    );
    assert_eq!(spaces_detail(&parse_mdf("")), "No spaces defined");
}

#[test]
fn a_malformed_line_is_a_bad_part_of_the_status_line() {
    let checks = status(&parse_mdf(
        "space linear 1 8\nchip broken\nchip ok 2 8 16 rom\n",
    ));
    assert!(checks[0].is_bad);
    assert!(checks[0].text.starts_with("Line 2: "), "{}", checks[0].text);
    let last = checks.last().expect("a summary");
    assert!(!last.is_bad);
    assert_eq!(last.text, "1 chip · 0 mappings");
}

#[test]
fn more_than_three_problems_are_counted() {
    let checks = status(&parse_mdf("a\nb\nc\nd\ne\n"));
    let texts: Vec<_> = checks.iter().map(|check| check.text.as_str()).collect();
    assert_eq!(texts.len(), 5);
    assert_eq!(texts[3], "+2 more");
}
