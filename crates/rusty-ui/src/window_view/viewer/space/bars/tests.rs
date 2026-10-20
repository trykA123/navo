use rusty_automotive::Section;
use rusty_automotive::space::{Kind, Region, RegionSource};

use super::{FileBytes, FilePart, percent_text, usage_label, usage_lines};

fn region(total: Option<u64>, used: u64) -> Region {
    let mut by_kind = [0; Kind::COUNT];
    by_kind[Kind::Code.index()] = used;
    Region {
        name: "pflash0".into(),
        start: Some(0x8000_0000),
        start_estimated: false,
        total,
        chip: None,
        used,
        by_kind,
        blocks: Vec::new(),
        source: if total.is_some() {
            RegionSource::Mdf
        } else {
            RegionSource::Clusters
        },
    }
}

fn section(name: &str, kind: u32, flags: u64, size: u64) -> Section {
    Section {
        name: name.into(),
        kind,
        flags,
        address: 0,
        size,
        offset: 0,
    }
}

#[test]
fn the_label_names_the_percent_and_the_free_bytes() {
    let label = usage_label(&region(Some(4096), 1024));
    assert_eq!(label, "pflash0: 25% used, 3.0 KB free");
}

#[test]
fn a_cluster_says_free_is_not_known() {
    let cluster = region(None, 2048);
    assert!(usage_label(&cluster).ends_with("free not known"));
    let lines = usage_lines(&cluster, None);
    assert_eq!(lines.len(), 2);
    assert!(lines[1].starts_with("free not known"));
    assert!(!lines[0].contains("free"));
}

#[test]
fn a_known_region_lists_used_free_and_total_in_one_line() {
    let lines = usage_lines(&region(Some(4096), 1024), None);
    assert_eq!(lines, ["1.0 KB used · 3.0 KB free · of 4.0 KB"]);
    let lines = usage_lines(&region(Some(4096), 1024), Some(512));
    assert!(lines[0].contains("512 B reserved"));
}

#[test]
fn small_shares_keep_a_decimal() {
    assert_eq!(percent_text(5, 1000), "0.5");
    assert_eq!(percent_text(500, 1000), "50");
    assert_eq!(percent_text(1, 0), "0.0");
}

#[test]
fn file_bytes_sort_sections_by_what_they_are() {
    let alloc = 2;
    let sections = [
        section("", 0, 0, 0),
        section(".text", 1, alloc | 4, 1000),
        section(".bss", 8, alloc | 1, 5000),
        section(".debug_info", 1, 0, 300),
        section(".note.build", 7, 0, 40),
        section(".symtab", 2, 0, 200),
        section(".strtab", 3, 0, 100),
        section(".comment", 1, 0, 10),
    ];
    let bytes = FileBytes::of(&sections);
    assert_eq!(bytes.bytes(FilePart::Loaded), 1000);
    assert_eq!(bytes.bytes(FilePart::Dwarf), 300);
    assert_eq!(bytes.bytes(FilePart::Notes), 40);
    assert_eq!(bytes.bytes(FilePart::Symbols), 300);
    assert_eq!(bytes.bytes(FilePart::Other), 10);
    assert_eq!(bytes.total(), 1650);
}
