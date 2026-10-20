#![allow(clippy::unwrap_used, reason = "synthetic test fixtures must parse")]

mod elf_build;

use std::sync::atomic::AtomicBool;

use elf_build::{EM_V800, Part, Spec, bss, build, data, parse, text};
use rusty_automotive::mdf::{ChipKind, RegionTable, TableOrigin, parse_mdf};
use rusty_automotive::space::{
    Kind, OwnerRef, RegionSource, Regions, regions_for_elf, regions_for_map,
};
use rusty_automotive::{Elf, MapModel, Section, index_map};

const ALLOC: u64 = 2;
const EXEC: u64 = 4;
const WRITE: u64 = 1;
const TLS: u64 = 0x400;
const PROGBITS: u32 = 1;
const NOTE: u32 = 7;
const NOBITS: u32 = 8;

const ROM: u64 = 0xA004_C000;
const ROM_SIZE: u64 = 960 * 1024;
const DSPR: u64 = 0x7000_0000;
const LMU: u64 = 0x9000_0000;

const MDF: &str = "# invented three-region part\n\
    space code 1 8\n\
    chip pflash 10 8 983040 rom\n\
    chip dspr 20 8 0x30000 ram\n\
    chip lmu 21 8 0x10000 ram\n\
    map 1 0xA004C000 983040 10 0\n\
    map 1 0x70000000 0x30000 20 0\n\
    map 1 0x90000000 0x10000 21 0\n";

fn mdf_table() -> RegionTable {
    parse_mdf(MDF).regions()
}

fn section(kind: u32, flags: u64) -> Section {
    Section {
        name: String::new(),
        kind,
        flags,
        address: 0,
        size: 1,
        offset: 0,
    }
}

fn rodata(address: u64, len: usize) -> Part {
    Part {
        name: ".rodata",
        flags: ALLOC,
        ..data(address, len)
    }
}

fn elf_of(parts: Vec<Part>) -> Elf {
    parse(&build(&Spec {
        is_64: false,
        is_big: false,
        machine: EM_V800,
        entry: 0,
        parts,
        symbols: None,
        loads: Vec::new(),
        debug: Vec::new(),
    }))
}

fn region<'a>(regions: &'a Regions, name: &str) -> &'a rusty_automotive::space::Region {
    regions.items.iter().find(|r| r.name == name).unwrap()
}

fn map_of(body: &str) -> MapModel {
    let text = format!("TASKING VX\n{body}");
    index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {}).unwrap()
}

const MAP: &str = "* Locate Result
| Section | Address | Size | Chip |
| .text.main | 0xa004c000 | 0x100 | mc0_rom |
| .rodata | 0xa004c100 | 32 | mc0_rom |
| .zdata | 0x70000000 | 16 | mc0_ram |
| .bss | 0x70000010 | 64 | mc0_ram |
| .mystery | 0x70000100 | 8 | mc0_ram |
* Memory Usage
| Memory | Code | Data | Reserved | Free | Total |
| mc0_rom | 288 | 0 | 0 | 3808 | 4096 |
| mc0_ram | 0 | 88 | 0 | 936 | 1024 |
";

const MAP_MDF: &str = "space code 1 8\n\
    chip MC0_ROM 1 8 4096 rom\n\
    chip MC0_RAM 2 8 1024 ram\n\
    map 1 0xA004C000 4096 1 0\n\
    map 1 0x70000000 1024 2 0\n";

#[test]
fn space_regions_kind_of_every_flag_mix() {
    let cases = [
        (PROGBITS, ALLOC | EXEC, Kind::Code),
        (PROGBITS, ALLOC | EXEC | WRITE, Kind::Code),
        (NOBITS, ALLOC | EXEC, Kind::Code),
        (NOBITS, ALLOC | WRITE, Kind::Bss),
        (NOBITS, ALLOC, Kind::Bss),
        (PROGBITS, ALLOC | WRITE, Kind::Data),
        (PROGBITS, ALLOC, Kind::Rodata),
        (NOTE, ALLOC, Kind::Rodata),
        (PROGBITS, 0, Kind::Other),
        (PROGBITS, EXEC | WRITE, Kind::Other),
        (NOBITS, WRITE, Kind::Other),
    ];
    for (kind, flags, expected) in cases {
        assert_eq!(
            Kind::of_section(&section(kind, flags)),
            expected,
            "type {kind} flags {flags:#x}"
        );
    }
}

#[test]
fn space_regions_kind_of_name_uses_the_leading_part() {
    let cases = [
        (".text", Kind::Code),
        (".text.main", Kind::Code),
        (".TEXT.Main", Kind::Code),
        (".rodata.str1.1", Kind::Rodata),
        (".zrodata", Kind::Rodata),
        (".data", Kind::Data),
        (".sdata.table", Kind::Data),
        (".zdata", Kind::Data),
        (".bss", Kind::Bss),
        (".sbss", Kind::Bss),
        (".zbss.stack", Kind::Bss),
        (".debug_info", Kind::Other),
        (".comment", Kind::Other),
        (".data_overlay", Kind::Other),
        (".", Kind::Other),
        ("", Kind::Other),
    ];
    for (name, expected) in cases {
        assert_eq!(Kind::of_name(name), expected, "{name}");
    }
}

#[test]
fn space_regions_mdf_gives_exact_regions_and_free_space() {
    let elf = elf_of(vec![
        text(ROM, 0x400),
        rodata(ROM + 0x400, 0x100),
        data(DSPR, 0x40),
        bss(DSPR + 0x40, 0x200),
    ]);
    let table = mdf_table();
    let regions = regions_for_elf(&elf, Some(&table));
    assert_eq!(regions.outside, 0);
    assert_eq!(regions.items.len(), 3);
    let flash = region(&regions, "pflash");
    assert_eq!(flash.source, RegionSource::Mdf);
    assert_eq!(
        (flash.start, flash.start_estimated, flash.total),
        (Some(ROM), false, Some(ROM_SIZE))
    );
    assert_eq!(flash.chip, Some(ChipKind::Rom));
    assert_eq!((flash.used, flash.free()), (0x500, Some(ROM_SIZE - 0x500)));
    assert_eq!(flash.by_kind[Kind::Code.index()], 0x400);
    assert_eq!(flash.by_kind[Kind::Rodata.index()], 0x100);
    let ram = region(&regions, "dspr");
    assert_eq!(ram.chip, Some(ChipKind::Ram));
    assert_eq!(ram.used, 0x240);
    assert_eq!(ram.by_kind[Kind::Data.index()], 0x40);
    assert_eq!(ram.by_kind[Kind::Bss.index()], 0x200);
    let empty = region(&regions, "lmu");
    assert_eq!((empty.used, empty.blocks.len()), (0, 0));
    assert_eq!(empty.free(), Some(0x10000));
}

#[test]
fn space_regions_blocks_follow_address_order_and_name_their_section() {
    let elf = elf_of(vec![
        rodata(ROM + 0x400, 0x100),
        text(ROM, 0x400),
        bss(DSPR, 0x10),
    ]);
    let table = mdf_table();
    let regions = regions_for_elf(&elf, Some(&table));
    let blocks = &region(&regions, "pflash").blocks;
    let seen: Vec<_> = blocks
        .iter()
        .map(|b| (b.start, b.size, b.kind, b.owner))
        .collect();
    assert_eq!(
        seen,
        vec![
            (ROM, 0x400, Kind::Code, OwnerRef::ElfSection(2)),
            (ROM + 0x400, 0x100, Kind::Rodata, OwnerRef::ElfSection(1)),
        ]
    );
    assert_eq!(elf.sections[2].name, ".text");
}

#[test]
fn space_regions_a_section_outside_every_region_is_counted() {
    let elf = elf_of(vec![
        text(ROM, 0x100),
        text(0x1234_0000, 0x100),
        data(LMU + 0x10000, 0x10),
    ]);
    let table = mdf_table();
    let regions = regions_for_elf(&elf, Some(&table));
    assert_eq!(regions.outside, 2);
    assert_eq!(region(&regions, "pflash").used, 0x100);
}

#[test]
fn space_regions_non_alloc_zero_size_and_tls_bss_are_not_placed() {
    let thread = Part {
        name: ".tbss",
        kind: NOBITS,
        flags: ALLOC | WRITE | TLS,
        ..bss(DSPR, 0x100)
    };
    let note = Part {
        name: ".note",
        kind: NOTE,
        flags: 0,
        ..text(ROM, 0x800)
    };
    let elf = elf_of(vec![thread, note, text(ROM, 0), data(DSPR, 0x10)]);
    let table = mdf_table();
    let regions = regions_for_elf(&elf, Some(&table));
    assert_eq!(regions.outside, 0);
    assert_eq!(region(&regions, "dspr").used, 0x10);
    assert_eq!(region(&regions, "pflash").used, 0);
}

#[test]
fn space_regions_a_spanning_section_stays_whole_in_its_first_region() {
    let table = parse_mdf(
        "space s 1 8\nchip a 1 8 0x1000 rom\nchip b 2 8 0x1000 rom\n\
         map 1 0x1000 0x1000 1 0\nmap 1 0x2000 0x1000 2 0\n",
    )
    .regions();
    let elf = elf_of(vec![text(0x1F00, 0x200)]);
    let regions = regions_for_elf(&elf, Some(&table));
    assert_eq!(regions.outside, 0);
    assert_eq!(region(&regions, "a").used, 0x200);
    assert_eq!(region(&regions, "a").blocks.len(), 1);
    assert_eq!(region(&regions, "b").used, 0);
}

#[test]
fn space_regions_without_siblings_addresses_form_clusters_with_no_free_figure() {
    let elf = elf_of(vec![
        text(0x0800_0000, 0x400),
        rodata(0x0800_0400, 0x100),
        data(0x2000_0000, 0x40),
        bss(0x2000_0040, 0x200),
    ]);
    for table in [None, Some(&RegionTable::default())] {
        let regions = regions_for_elf(&elf, table);
        assert_eq!(regions.outside, 0);
        assert_eq!(regions.items.len(), 2);
        let flash = &regions.items[0];
        assert_eq!(flash.name, "0x08000000");
        assert_eq!(flash.source, RegionSource::Clusters);
        assert_eq!(
            (flash.start, flash.start_estimated),
            (Some(0x0800_0000), true)
        );
        assert_eq!((flash.total, flash.free(), flash.chip), (None, None, None));
        assert_eq!(flash.used, 0x500);
        let ram = &regions.items[1];
        assert_eq!(
            (ram.start, ram.used, ram.total),
            (Some(0x2000_0000), 0x240, None)
        );
    }
}

#[test]
fn space_regions_clusters_split_at_sixteen_mebibytes() {
    let mib16 = 16 * 1024 * 1024;
    let joined = elf_of(vec![
        text(0x1000, 0x100),
        text(0x1000 + 0x100 + mib16 - 1, 0x10),
    ]);
    assert_eq!(regions_for_elf(&joined, None).items.len(), 1);
    let split = elf_of(vec![
        text(0x1000, 0x100),
        text(0x1000 + 0x100 + mib16, 0x10),
    ]);
    let regions = regions_for_elf(&split, None);
    assert_eq!(regions.items.len(), 2);
    assert_eq!(regions.items[1].start, Some(0x1000 + 0x100 + mib16));
}

#[test]
fn space_regions_an_elf_without_loaded_sections_has_no_regions() {
    let note = Part {
        name: ".note",
        kind: NOTE,
        flags: 0,
        ..text(0, 0x10)
    };
    let regions = regions_for_elf(&elf_of(vec![note]), None);
    assert!(regions.items.is_empty());
    assert_eq!(regions.outside, 0);
}

#[test]
fn space_regions_per_function_sections_fill_the_flash_region() {
    let functions = 3_300;
    let mut parts: Vec<Part> = (0..functions)
        .map(|at| text(ROM + at as u64 * 0x100, 0x100))
        .collect();
    parts.push(data(DSPR, 0x400));
    parts.push(bss(DSPR + 0x400, 0x800));
    let elf = elf_of(parts);
    let table = mdf_table();
    let regions = regions_for_elf(&elf, Some(&table));
    let flash = region(&regions, "pflash");
    assert_eq!(flash.blocks.len(), functions);
    assert_eq!(flash.used, functions as u64 * 0x100);
    assert_eq!(flash.by_kind[Kind::Code.index()], flash.used);
    assert!(
        flash
            .blocks
            .windows(2)
            .all(|pair| pair[0].start < pair[1].start)
    );
    assert_eq!(flash.free(), Some(ROM_SIZE - flash.used));
    assert_eq!(region(&regions, "dspr").used, 0xC00);
    assert_eq!(regions.outside, 0);
    let clusters = regions_for_elf(&elf, None);
    assert_eq!(clusters.items.len(), 2);
    assert_eq!(clusters.items[1].used, flash.used);
}

#[test]
fn space_regions_merged_sections_with_many_small_stacks() {
    let mut parts = vec![text(ROM, 0x8_0000), rodata(ROM + 0x8_0000, 0x1_0000)];
    parts.push(data(DSPR, 0x2000));
    parts.extend((0..108).map(|at| bss(DSPR + 0x2000 + at * 0x100, 0x100)));
    let elf = elf_of(parts);
    assert_eq!(
        elf.sections.iter().filter(|s| s.is_loaded()).count(),
        111,
        "fixture size"
    );
    let table = mdf_table();
    let regions = regions_for_elf(&elf, Some(&table));
    let flash = region(&regions, "pflash");
    assert_eq!((flash.blocks.len(), flash.used), (2, 0x9_0000));
    let ram = region(&regions, "dspr");
    assert_eq!(ram.blocks.len(), 109);
    assert_eq!(ram.by_kind[Kind::Bss.index()], 108 * 0x100);
    assert_eq!(ram.by_kind[Kind::Data.index()], 0x2000);
    assert_eq!(ram.used, 0x2000 + 108 * 0x100);
    assert_eq!(regions.outside, 0);
}

#[test]
fn space_regions_map_regions_estimate_a_missing_start_from_the_lowest_section() {
    let map = map_of(MAP);
    let table = RegionTable::from_map(&map);
    assert_eq!(table.origin, TableOrigin::Map);
    let regions = regions_for_map(&map, None);
    assert_eq!(regions.outside, 0);
    let names: Vec<_> = regions.items.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["mc0_rom", "mc0_ram"]);
    let rom = &regions.items[0];
    assert_eq!(rom.source, RegionSource::Map);
    assert_eq!(
        (rom.start, rom.start_estimated, rom.total),
        (Some(0xA004_C000), true, Some(4096))
    );
    assert_eq!((rom.used, rom.free()), (288, Some(3808)));
    assert_eq!(rom.by_kind[Kind::Code.index()], 256);
    assert_eq!(rom.by_kind[Kind::Rodata.index()], 32);
    let ram = &regions.items[1];
    assert_eq!(ram.by_kind[Kind::Data.index()], 16);
    assert_eq!(ram.by_kind[Kind::Bss.index()], 64);
    assert_eq!(ram.by_kind[Kind::Other.index()], 8);
    assert_eq!(ram.blocks[0].owner, OwnerRef::MapSection(2));
}

#[test]
fn space_regions_map_start_comes_from_the_mdf_by_case_insensitive_name() {
    let map = map_of(MAP);
    let mdf = parse_mdf(MAP_MDF).regions();
    let merged = RegionTable::merge(&map, &mdf);
    assert_eq!(merged.origin, TableOrigin::Map);
    let rom = &merged.regions[0];
    assert_eq!(rom.name, "mc0_rom");
    assert_eq!(
        (rom.start, rom.start_estimated, rom.kind),
        (Some(0xA004_C000), false, Some(ChipKind::Rom))
    );
    let regions = regions_for_map(&map, Some(&mdf));
    assert_eq!(regions.items[1].chip, Some(ChipKind::Ram));
    assert_eq!(regions.items[1].start, Some(DSPR));
    assert!(!regions.items[1].start_estimated);
    assert_eq!(regions.items[0].used, 288);
}

#[test]
fn space_regions_elf_uses_map_regions_and_mdf_starts_when_both_are_beside_it() {
    let map = map_of(MAP);
    let mdf = parse_mdf(MAP_MDF).regions();
    let table = RegionTable::merge(&map, &mdf);
    let elf = elf_of(vec![text(ROM, 0x200), data(DSPR, 0x20), text(LMU, 0x10)]);
    let regions = regions_for_elf(&elf, Some(&table));
    assert_eq!(regions.outside, 1);
    let rom = region(&regions, "mc0_rom");
    assert_eq!(
        (rom.source, rom.used, rom.free()),
        (RegionSource::Map, 0x200, Some(4096 - 0x200))
    );
    assert_eq!(region(&regions, "mc0_ram").used, 0x20);
}

#[test]
fn space_regions_map_without_a_memory_table_uses_the_mdf_then_clusters() {
    let body = "* Locate Result\n| Section | Address | Size | Chip |\n\
        | .text | 0xa004c000 | 0x100 | mc0_rom |\n| .bss | 0x70000000 | 8 | mc0_ram |\n";
    let map = map_of(body);
    assert!(map.regions.is_empty());
    let mdf = parse_mdf(MAP_MDF).regions();
    let from_mdf = regions_for_map(&map, Some(&mdf));
    assert_eq!(from_mdf.items.len(), 2);
    assert_eq!(from_mdf.items[0].source, RegionSource::Mdf);
    assert_eq!(from_mdf.items[0].used, 0x100);
    let clusters = regions_for_map(&map, None);
    assert_eq!(clusters.items.len(), 2);
    assert_eq!(clusters.items[0].source, RegionSource::Clusters);
    assert_eq!(clusters.items[0].total, None);
}

#[test]
fn space_regions_map_section_without_a_region_falls_back_to_its_address() {
    let body = "* Locate Result\n| Section | Address | Size |\n\
        | .text | 0x100 | 0x40 |\n| .zbss | 0x9000 | 8 |\n\
        * Memory Usage\n| Memory | Code | Data | Reserved | Free | Total |\n\
        | rom | 64 | 0 | 0 | 192 | 256 |\n";
    let map = map_of(body);
    let mdf = parse_mdf("space s 1 8\nchip rom 1 8 256 rom\nmap 1 0x100 256 1 0\n").regions();
    let regions = regions_for_map(&map, Some(&mdf));
    assert_eq!(regions.items[0].used, 0x40);
    assert_eq!(regions.outside, 1);
}
