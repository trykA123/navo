use rusty_automotive::Section;
use rusty_automotive::space::{Kind, Region, RegionSource};

use super::{FileBytes, FilePart, Part, bar_parts, percent_text, usage_label, usage_lines};

fn region(total: Option<u64>, used: u64) -> Region {
    let mut by_kind = [0; Kind::COUNT];
    by_kind[Kind::Code.index()] = used;
    Region {
        name: "pflash0".into(),
        start: Some(0x8000_0000),
        start_estimated: false,
        total,
        chip: None,
        reserved: None,
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
fn zero_reserved_bytes_are_not_named() {
    let lines = usage_lines(&region(Some(4096), 1024), Some(0));
    assert_eq!(lines, ["1.0 KB used · 3.0 KB free · of 4.0 KB"]);
}

#[test]
fn percent_never_reads_fuller_or_emptier_than_it_is() {
    assert_eq!(percent_text(0, 1000), "0");
    assert_eq!(percent_text(1, 1000), "<1");
    assert_eq!(percent_text(9, 1000), "<1");
    assert_eq!(percent_text(10, 1000), "1");
    assert_eq!(percent_text(990, 1000), "99");
    assert_eq!(percent_text(995, 1000), "99");
    assert_eq!(percent_text(9999, 10_000), "99");
    assert_eq!(percent_text(1000, 1000), "100");
}

#[test]
fn a_nearly_full_region_says_99_with_a_sliver_free() {
    let label = usage_label(&region(Some(10_000), 9_999));
    assert!(label.contains("99% used"), "{label}");
}

#[test]
fn reserved_bytes_get_their_own_segment_after_used() {
    let mut reserved = region(Some(1000), 250);
    reserved.reserved = Some(500);
    let parts = bar_parts(&reserved);
    assert_eq!(parts, [(Part::Kind(0), 0.25), (Part::Reserved, 0.5)]);
    let free = reserved.free().unwrap() as f64 / 1000.;
    assert_eq!(parts.iter().map(|(_, share)| share).sum::<f64>() + free, 1.);
}

#[test]
fn no_reserved_bytes_draw_no_reserved_segment() {
    let parts = bar_parts(&region(Some(1000), 250));
    assert!(parts.iter().all(|(part, _)| *part != Part::Reserved));
}

#[test]
fn a_cluster_bar_is_all_composition_with_no_free_share() {
    let mut cluster = region(None, 2000);
    cluster.by_kind[Kind::Rodata.index()] = 2000;
    cluster.by_kind[Kind::Code.index()] = 6000;
    let parts = bar_parts(&cluster);
    assert_eq!(parts, [(Part::Kind(0), 0.75), (Part::Kind(1), 0.25)]);
}

#[test]
fn a_zero_size_region_shows_the_used_size_and_no_percent() {
    let zero = region(Some(0), 2048);
    assert_eq!(usage_lines(&zero, None), ["2.0 KB used"]);
    let label = usage_label(&zero);
    assert!(
        !label.contains('%') && !label.contains("not known"),
        "{label}"
    );
    assert_eq!(bar_parts(&zero), [(Part::Kind(0), 1.)]);
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

#[test]
fn bar_parts_of_near_u64_max_kinds_stay_within_the_bar() {
    let mut huge = region(Some(u64::MAX), u64::MAX - 1);
    huge.by_kind[Kind::Data.index()] = u64::MAX - 1;
    huge.by_kind[Kind::Fill.index()] = u64::MAX - 1;
    huge.reserved = Some(u64::MAX);
    let parts = bar_parts(&huge);
    assert_eq!(parts.len(), 4);
    let total: f64 = parts.iter().map(|(_, share)| share).sum();
    assert!(total <= 4.0 && parts.iter().all(|(_, share)| share.is_finite()));
}
