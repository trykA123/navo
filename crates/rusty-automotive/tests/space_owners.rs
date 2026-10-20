#![allow(clippy::unwrap_used, reason = "synthetic test fixtures must parse")]

mod elf_build;

use std::sync::atomic::AtomicBool;

use elf_build::{EM_V800, Named, Part, Spec, bss, build, data, func, object, parse, read, text};
use object::elf as raw;
use rusty_automotive::mdf::{RegionTable, parse_mdf};
use rusty_automotive::space::{
    Kind, NOT_ATTRIBUTED, NameRef, Owner, Owners, Regions, owners, regions_for_elf,
};
use rusty_automotive::{Elf, Lines, MapModel, index_map};

const ROM: u64 = 0xA004_C000;
const DSPR: u64 = 0x7000_0000;
const LOAD_RX: u32 = raw::PF_R | raw::PF_X;
const LOAD_RW: u32 = raw::PF_R | raw::PF_W;

const MDF: &str = "space code 1 8\n\
    chip pflash 10 8 0xF0000 rom\n\
    chip dspr 20 8 0x30000 ram\n\
    map 1 0xA004C000 0xF0000 10 0\n\
    map 1 0x70000000 0x30000 20 0\n";

fn table() -> RegionTable {
    parse_mdf(MDF).regions()
}

fn spec(parts: Vec<Part>, symbols: Option<Vec<Named>>, loads: Vec<(usize, u32)>) -> Spec {
    Spec {
        is_64: false,
        is_big: false,
        machine: EM_V800,
        entry: 0,
        parts,
        symbols,
        loads,
        debug: Vec::new(),
    }
}

fn named_text(name: &'static str, address: u64, len: usize) -> Part {
    Part {
        name,
        ..text(address, len)
    }
}

fn local(mut symbol: Named) -> Named {
    symbol.bind = raw::STB_LOCAL;
    symbol
}

fn file(name: &str) -> Named {
    Named {
        kind: raw::STT_FILE,
        bind: raw::STB_LOCAL,
        part: None,
        ..func(name, 0, 0, 0)
    }
}

fn sum(items: &[Owner], image: bool) -> u64 {
    items
        .iter()
        .filter(|owner| owner.image == image)
        .map(|owner| owner.size)
        .sum()
}

fn names<'a>(owners: &Owners, elf: &'a Elf, items: &[Owner]) -> Vec<&'a str> {
    items
        .iter()
        .map(|owner| owners.name(owner, elf, None))
        .collect()
}

fn assert_tiles_regions(owners: &Owners, regions: &Regions) {
    for region in &regions.items {
        let start = region.start.unwrap();
        let span = match region.total {
            Some(total) => start..start + total,
            None => {
                start
                    ..region
                        .blocks
                        .iter()
                        .map(|b| b.start + b.size)
                        .max()
                        .unwrap()
            }
        };
        let inside = owners.in_span(span);
        let all: u64 = inside.iter().map(|owner| owner.size).sum();
        assert_eq!(all, region.used, "{}", region.name);
    }
}

fn region_named<'a>(regions: &'a Regions, name: &str) -> &'a rusty_automotive::space::Region {
    regions.items.iter().find(|r| r.name == name).unwrap()
}

fn map_of(body: &str) -> MapModel {
    let text = format!("TASKING VX\n{body}");
    index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {}).unwrap()
}

#[test]
fn space_owners_per_function_sections_own_their_bytes_by_name() {
    let elf = parse(&build(&spec(
        vec![
            named_text(".text.Com_Init", ROM, 0x40),
            named_text(".text.Com_Main", ROM + 0x40, 0x80),
            named_text(".text.Lamp_Read", ROM + 0xC0, 0x20),
            data(DSPR, 0x10),
        ],
        Some(vec![
            func("Com_Init", ROM, 0x40, 0),
            func("Com_Main", ROM + 0x40, 0x80, 1),
            func("Lamp_Read", ROM + 0xC0, 0x20, 2),
            object("lamp_state", DSPR, 4, 3),
        ]),
        Vec::new(),
    )));
    let found = owners(&elf, None, None);
    let flash = found.in_span(ROM..ROM + 0xE0);
    assert_eq!(
        names(&found, &elf, flash),
        [".text.Com_Init", ".text.Com_Main", ".text.Lamp_Read"]
    );
    assert!(
        flash
            .iter()
            .all(|owner| matches!(owner.name, NameRef::Section(_)))
    );
    assert_eq!(found.provenance.from_sections, 3);
    assert_eq!(found.provenance.from_symbols, 1);
    assert_eq!(found.provenance.not_attributed, 1);
    assert_tiles_regions(&found, &regions_for_elf(&elf, Some(&table())));
}

#[test]
fn space_owners_merged_text_uses_sized_symbols_and_skips_local_labels() {
    let elf = parse(&build(&spec(
        vec![text(ROM, 0x100)],
        Some(vec![
            func("Com_Init", ROM, 0x40, 0),
            local(func(".L12", ROM + 0x20, 0, 0)),
            func("Com_Main", ROM + 0x40, 0x20, 0),
            func("asm_entry", ROM + 0x60, 0, 0),
            func("Lamp_Read", ROM + 0x80, 0x10, 0),
        ]),
        vec![(0, LOAD_RX)],
    )));
    let found = owners(&elf, None, None);
    let items = found.in_span(ROM..ROM + 0x100);
    assert_eq!(
        names(&found, &elf, items),
        [
            "Com_Init",
            "Com_Main",
            "asm_entry",
            "Lamp_Read",
            NOT_ATTRIBUTED
        ]
    );
    let sizes: Vec<_> = items.iter().map(|owner| owner.size).collect();
    assert_eq!(sizes, [0x40, 0x20, 0x20, 0x10, 0x70]);
    let estimated: Vec<_> = items.iter().map(|owner| owner.estimated).collect();
    assert_eq!(estimated, [false, false, true, false, false]);
    assert!(items.iter().all(|owner| owner.kind == Kind::Code));
    assert_eq!(sum(items, false), 0x100);
    let provenance = &found.provenance;
    assert_eq!(
        (
            provenance.from_symbols,
            provenance.estimated,
            provenance.not_attributed
        ),
        (3, 1, 1)
    );
    assert_eq!(
        (provenance.has_symbols, provenance.has_lines),
        (true, false)
    );
}

#[test]
fn space_owners_zero_size_labels_alone_never_split_a_section() {
    let elf = parse(&build(&spec(
        vec![text(ROM, 0x40)],
        Some(vec![
            local(func(".L1", ROM, 0, 0)),
            local(func(".L2", ROM + 0x10, 0, 0)),
            local(func(".L3", ROM + 0x20, 0, 0)),
        ]),
        Vec::new(),
    )));
    let found = owners(&elf, None, None);
    assert_eq!(found.items.len(), 1);
    assert_eq!(found.items[0].name, NameRef::Section(1));
    assert_eq!(found.items[0].size, 0x40);
    assert_eq!(found.provenance.from_sections, 1);
}

#[test]
fn space_owners_a_stripped_file_falls_back_to_section_owners() {
    let elf = parse(&build(&spec(
        vec![
            text(ROM, 0x400),
            Part {
                name: ".rodata",
                flags: 2,
                ..data(ROM + 0x400, 0x80)
            },
            bss(DSPR, 0x200),
        ],
        None,
        Vec::new(),
    )));
    let found = owners(&elf, None, None);
    assert_eq!(found.items.len(), 3);
    assert_eq!(
        names(&found, &elf, &found.items),
        [".bss", ".text", ".rodata"]
    );
    let kinds: Vec<_> = found.items.iter().map(|owner| owner.kind).collect();
    assert_eq!(kinds, [Kind::Bss, Kind::Code, Kind::Rodata]);
    let provenance = &found.provenance;
    assert!(!provenance.has_symbols);
    assert_eq!(provenance.from_sections, 3);
    assert_eq!(provenance.from_symbols + provenance.estimated, 0);
    assert_tiles_regions(&found, &regions_for_elf(&elf, Some(&table())));
}

#[test]
fn space_owners_fill_sections_are_never_owners_and_never_unattributed() {
    let fill = |address, len| Part {
        name: "$fill",
        flags: 2,
        ..data(address, len)
    };
    let elf = parse(&build(&spec(
        vec![
            text(ROM, 0x100),
            fill(ROM + 0x100, 0x80),
            fill(ROM + 0x180, 0x40),
        ],
        Some(vec![func("main", ROM, 0x100, 0)]),
        vec![(0, LOAD_RX)],
    )));
    let found = owners(&elf, None, None);
    let regions = regions_for_elf(&elf, Some(&table()));
    assert_tiles_regions(&found, &regions);
    let flash = region_named(&regions, "pflash");
    assert_eq!(
        (flash.used, flash.by_kind[Kind::Fill.index()]),
        (0x100, 0xC0)
    );
    assert_eq!(found.items.len(), 1);
    assert!(found.items.iter().all(|owner| owner.kind != Kind::Fill));
    assert_eq!(found.provenance.not_attributed, 0);
    assert_eq!(names(&found, &elf, &found.items), ["main"]);
}

#[test]
fn space_owners_data_counts_in_rom_and_in_ram() {
    let image = ROM + 0x100;
    let init = Part {
        load_address: image,
        ..data(DSPR, 0x40)
    };
    let elf = parse(&build(&spec(
        vec![text(ROM, 0x100), init, bss(DSPR + 0x40, 0x80)],
        Some(vec![
            func("main", ROM, 0x100, 0),
            object("counter", DSPR, 4, 1),
            object("limit", DSPR + 0x10, 8, 1),
            object("scratch", DSPR + 0x40, 0x80, 2),
        ]),
        vec![(0, LOAD_RX), (1, LOAD_RW), (2, LOAD_RW)],
    )));
    let found = owners(&elf, None, None);
    let regions = regions_for_elf(&elf, Some(&table()));
    assert_tiles_regions(&found, &regions);
    let flash = found.in_span(ROM..ROM + 0xF0000);
    assert_eq!(sum(flash, false), 0x100);
    assert_eq!(sum(flash, true), 0x40);
    let copies: Vec<Owner> = flash.iter().filter(|owner| owner.image).copied().collect();
    let spans: Vec<_> = copies
        .iter()
        .map(|owner| (owner.start, owner.size))
        .collect();
    assert_eq!(
        spans,
        [
            (image, 4),
            (image + 4, 0xC),
            (image + 0x10, 8),
            (image + 0x18, 0x28)
        ]
    );
    assert!(copies.iter().all(|owner| owner.kind == Kind::Data));
    assert_eq!(
        names(&found, &elf, &copies),
        ["counter", NOT_ATTRIBUTED, "limit", NOT_ATTRIBUTED]
    );
    let ram = found.in_span(DSPR..DSPR + 0x30000);
    assert_eq!(sum(ram, true), 0);
    assert_eq!(sum(ram, false), 0x40 + 0x80);
    assert_eq!(found.provenance.load_images, 1);
}

#[test]
fn space_owners_data_without_a_load_address_has_no_image() {
    let elf = parse(&build(&spec(
        vec![text(ROM, 0x40), data(DSPR, 0x40)],
        Some(vec![object("counter", DSPR, 4, 1)]),
        vec![(0, LOAD_RX), (1, LOAD_RW)],
    )));
    let found = owners(&elf, None, None);
    assert!(found.items.iter().all(|owner| !owner.image));
    assert_eq!(found.provenance.load_images, 0);
}

#[test]
fn space_owners_aliases_and_nested_symbols_do_not_double_count() {
    let elf = parse(&build(&spec(
        vec![data(DSPR, 0x40)],
        Some(vec![
            object("table", DSPR, 0x20, 0),
            object("table_alias", DSPR, 0x20, 0),
            local(object("table_row", DSPR + 8, 8, 0)),
            object("tail", DSPR + 0x30, 0x40, 0),
        ]),
        Vec::new(),
    )));
    let found = owners(&elf, None, None);
    assert_eq!(sum(&found.items, false), 0x40);
    assert_eq!(
        names(&found, &elf, &found.items),
        ["table", NOT_ATTRIBUTED, "tail"]
    );
}

#[test]
fn space_owners_dwarf_lines_and_file_symbols_name_source_and_object() {
    let rows = [(ROM, 0x40, 7), (ROM + 0x40, 0x40, 9)];
    let mut file_spec = spec(
        vec![text(ROM, 0x80)],
        Some(vec![
            file("Com_Main.o"),
            local(func("Com_Init", ROM, 0x40, 0)),
            file("lamp.c"),
            local(func("Lamp_Read", ROM + 0x40, 0x40, 0)),
        ]),
        Vec::new(),
    );
    file_spec.debug = elf_build::line_program(false, false, &rows);
    let (elf, lines) = read(&build(&file_spec)).unwrap();
    let with_lines = owners(&elf, Some(&lines), None);
    let items = with_lines.in_span(ROM..ROM + 0x80);
    assert!(with_lines.provenance.has_lines);
    assert_eq!(with_lines.object(&items[0]), Some("Com_Main.o"));
    assert_eq!(with_lines.object(&items[1]), None);
    assert!(
        with_lines
            .source_file(&items[0])
            .unwrap()
            .ends_with("main.c")
    );
    assert!(
        with_lines
            .source_file(&items[1])
            .unwrap()
            .ends_with("main.c")
    );

    let without = owners(&elf, None, None);
    let items = without.in_span(ROM..ROM + 0x80);
    assert_eq!(without.source_file(&items[0]), None);
    assert_eq!(without.source_file(&items[1]), Some("lamp.c"));
}

#[test]
fn space_owners_compiler_temp_files_name_nothing() {
    let elf = parse(&build(&spec(
        vec![text(ROM, 0x40)],
        Some(vec![
            file("ctc12345.src"),
            local(func("helper", ROM, 0x40, 0)),
        ]),
        Vec::new(),
    )));
    let found = owners(&elf, None, None);
    assert_eq!(found.object(&found.items[0]), None);
    assert_eq!(found.source_file(&found.items[0]), None);
}

const MAP: &str = "* Link Result
| [in] | | | [out] | | |
| File | Section | Size | Offset | Section | Size |
| lamp.o | .text.Lamp_Read (1) | 0x20 | 0x0 | .text (2) | 0x60 |
| com.o | .text.Com_Main (3) | 0x40 | 0x20 | .text (2) | 0x60 |
* Locate Result
* Sections
+ Space chip:core:linear (MAU = 8bit)
+-------------------------+
| Group | Chip | Section | Space addr | Chip addr | Size (MAU) | Alignment |
|=======|======|=========|============|===========|============|===========|
| code | mc0_rom | .text (2) | 0xa004c000 | 0x0 | 0x60 | 0x4 |
* Memory Usage
| Memory | Code | Data | Reserved | Free | Total |
| mc0_rom | 96 | 0 | 0 | 928 | 1024 |
";

#[test]
fn space_owners_a_sibling_map_gives_exact_object_files() {
    let elf = parse(&build(&spec(
        vec![text(ROM, 0x80)],
        Some(vec![
            func("Lamp_Read", ROM, 0x20, 0),
            func("Com_Main", ROM + 0x20, 0x40, 0),
        ]),
        Vec::new(),
    )));
    let map = map_of(MAP);
    assert_eq!(map.contributions.len(), 2);
    let found = owners(&elf, None, Some(&map));
    let items = found.in_span(ROM..ROM + 0x80);
    let seen: Vec<_> = items
        .iter()
        .map(|owner| {
            (
                found.name(owner, &elf, Some(&map)),
                found.object(owner),
                owner.size,
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            (".text.Lamp_Read", Some("lamp.o"), 0x20),
            (".text.Com_Main", Some("com.o"), 0x40),
            (NOT_ATTRIBUTED, None, 0x20),
        ]
    );
    assert_eq!(found.provenance.from_map, 2);
    assert_eq!(found.provenance.from_symbols, 0);
}

#[test]
fn space_owners_a_map_for_other_sections_is_ignored() {
    let elf = parse(&build(&spec(
        vec![named_text(".init", ROM, 0x80)],
        Some(vec![func("start", ROM, 0x80, 0)]),
        Vec::new(),
    )));
    let map = map_of(MAP);
    let found = owners(&elf, None, Some(&map));
    assert_eq!(found.provenance.from_map, 0);
    assert_eq!(found.object(&found.items[0]), None);
}

#[test]
fn space_owners_no_loaded_sections_gives_no_owners() {
    let note = Part {
        name: ".note",
        kind: 7,
        flags: 0,
        ..text(0, 0x10)
    };
    let elf = parse(&build(&spec(vec![note], None, Vec::new())));
    let found = owners(&elf, None::<&Lines>, None);
    assert!(found.items.is_empty());
}

#[test]
fn space_owners_a_load_address_that_overflows_gives_no_image() {
    let init = Part {
        load_address: 0xFFFF_FFFF_FFFF_FF00,
        ..data(0x1000, 0x200)
    };
    let mut wide = spec(
        vec![init],
        Some(vec![object("counter", 0x1000, 4, 0)]),
        vec![(0, LOAD_RW)],
    );
    wide.is_64 = true;
    let elf = parse(&build(&wide));
    let found = owners(&elf, None, None);
    assert!(found.items.iter().all(|owner| !owner.image));
    assert!(found.items.iter().all(|owner| owner.start < 0x2000));
    assert_eq!(found.provenance.load_images, 0);
    let regions = regions_for_elf(&elf, None);
    assert_eq!(regions.items.len(), 1);
    assert_eq!(regions.items[0].used, 0x200);
}
