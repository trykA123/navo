#![allow(clippy::unwrap_used, reason = "synthetic test fixtures must parse")]

mod elf_build;

use std::sync::atomic::AtomicBool;

use elf_build::{EM_V800, Named, Part, Spec, bss, build, data, func, object, parse, text};
use object::elf as raw;
use rusty_automotive::mdf::{RegionTable, parse_mdf};
use rusty_automotive::space::{
    Group, GroupBy, Grouping, Kind, NOT_ATTRIBUTED, OTHER, Origin, Region, Regions, TOP_GROUPS,
    available, default_grouping, flash_region, group, lit_blocks, owners, owners_of_map,
    regions_for_elf, regions_for_map, top,
};
use rusty_automotive::{Elf, MapModel, index_map};

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

fn elf_of(parts: Vec<Part>, symbols: Option<Vec<Named>>, loads: Vec<(usize, u32)>) -> Elf {
    parse(&build(&Spec {
        is_64: false,
        is_big: false,
        machine: EM_V800,
        entry: 0,
        parts,
        symbols,
        loads,
        debug: Vec::new(),
    }))
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

fn map_of(body: &str) -> MapModel {
    let text = format!("TASKING VX\n{body}");
    index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {}).unwrap()
}

fn region_named<'a>(regions: &'a Regions, name: &str) -> &'a Region {
    regions.items.iter().find(|r| r.name == name).unwrap()
}

fn by(grouping: Grouping) -> GroupBy<'static> {
    GroupBy {
        grouping,
        modules: &[],
    }
}

fn pairs(groups: &[Group]) -> Vec<(&str, u64)> {
    groups
        .iter()
        .map(|group| (group.name.as_str(), group.bytes))
        .collect()
}

fn module_elf() -> Elf {
    elf_of(
        vec![text(ROM, 0x200)],
        Some(vec![
            func("_start", ROM, 0x10, 0),
            local(func(".L12", ROM + 0x10, 0x10, 0)),
            func("Rte_Lamp_Read", ROM + 0x20, 0x20, 0),
            func("Rte_Lamp_Write", ROM + 0x40, 0x20, 0),
            func("SchM_Enter_Dio", ROM + 0x60, 0x10, 0),
            func("Dio_Cfg_Init", ROM + 0x70, 0x10, 0),
            func("Dio_Read", ROM + 0x80, 0x10, 0),
            file("Com_Main.o"),
            local(func("Com_Main", ROM + 0x90, 0x40, 0)),
            local(func("init", ROM + 0xD0, 0x20, 0)),
        ]),
        vec![(0, LOAD_RX)],
    )
}

fn flash_groups(elf: &Elf, grouping: Grouping, modules: &[String]) -> Vec<Group> {
    let found = owners(elf, None, None);
    let regions = regions_for_elf(elf, Some(&table()));
    let region = region_named(&regions, "pflash");
    let by = GroupBy { grouping, modules };
    group(&found, Origin::Elf { elf, map: None }, region, &by)
}

#[test]
fn space_groups_module_rule_uses_the_prefix_the_autosar_pair_and_drops_labels() {
    let elf = module_elf();
    let groups = flash_groups(&elf, Grouping::Module, &[]);
    assert_eq!(
        pairs(&groups),
        [
            (NOT_ATTRIBUTED, 0x200 - 0xF0),
            ("Com", 0x60),
            ("Rte_Lamp", 0x40),
            ("Dio", 0x20),
            (OTHER, 0x10),
            ("SchM_Enter", 0x10),
            ("start", 0x10),
        ]
    );
    assert!(groups.iter().all(|group| !group.listed));
}

#[test]
fn space_groups_the_users_list_wins_with_the_longest_prefix() {
    let elf = module_elf();
    let list = ["dio".to_owned(), "Dio_Cfg".to_owned(), "Rte".to_owned()];
    let groups = flash_groups(&elf, Grouping::Module, &list);
    let listed: Vec<_> = groups
        .iter()
        .filter(|group| group.listed)
        .map(|group| (group.name.as_str(), group.bytes))
        .collect();
    assert_eq!(listed, [("Rte", 0x40), ("Dio", 0x10), ("Dio_Cfg", 0x10)]);
    assert!(
        groups
            .iter()
            .any(|group| group.name == "Com" && !group.listed)
    );
}

#[test]
fn space_groups_a_listed_name_must_end_at_a_separator() {
    let elf = elf_of(
        vec![text(ROM, 0x40)],
        Some(vec![
            func("Comx_Run", ROM, 0x20, 0),
            func("Com", ROM + 0x20, 0x20, 0),
        ]),
        vec![(0, LOAD_RX)],
    );
    let list = ["Com".to_owned()];
    let groups = flash_groups(&elf, Grouping::Module, &list);
    assert_eq!(pairs(&groups), [("Com", 0x20), ("Comx", 0x20)]);
    assert!(groups[0].listed);
}

#[test]
fn space_groups_names_ignore_case_and_keep_the_first_spelling() {
    let elf = elf_of(
        vec![text(ROM, 0x40)],
        Some(vec![
            func("COM_Init", ROM, 0x20, 0),
            func("com_Main", ROM + 0x20, 0x20, 0),
        ]),
        vec![(0, LOAD_RX)],
    );
    let groups = flash_groups(&elf, Grouping::Module, &[]);
    assert_eq!(pairs(&groups), [("COM", 0x40)]);
    assert_eq!(groups[0].items, 2);
}

#[test]
fn space_groups_per_function_sections_name_the_module_without_the_kind_part() {
    let part = |name, at| Part {
        name,
        ..text(at, 0x20)
    };
    let elf = elf_of(
        vec![
            part(".text.Lamp.Read", ROM),
            part(".text.Lamp_Write", ROM + 0x20),
            part(".text.Com_Main", ROM + 0x40),
            part(".text", ROM + 0x60),
        ],
        None,
        Vec::new(),
    );
    let groups = flash_groups(&elf, Grouping::Module, &[]);
    assert_eq!(
        pairs(&groups),
        [("Lamp", 0x40), (OTHER, 0x20), ("Com", 0x20)]
    );
}

#[test]
fn space_groups_lit_blocks_mark_only_blocks_holding_the_group() {
    let elf = elf_of(
        vec![
            text(ROM, 0x100),
            Part {
                name: ".rodata",
                flags: 2,
                ..data(ROM + 0x100, 0x20)
            },
        ],
        Some(vec![func("Lamp_Read", ROM, 0x40, 0)]),
        vec![(0, LOAD_RX)],
    );
    let found = owners(&elf, None, None);
    let regions = regions_for_elf(&elf, Some(&table()));
    let region = region_named(&regions, "pflash");
    let origin = Origin::Elf {
        elf: &elf,
        map: None,
    };
    let by = by(Grouping::Section);
    assert_eq!(
        lit_blocks(&found, origin, region, (&by, ".RODATA")),
        [false, true]
    );
    let by_module = GroupBy {
        grouping: Grouping::Module,
        modules: &[],
    };
    assert_eq!(
        lit_blocks(&found, origin, region, (&by_module, "Lamp")),
        [true, false]
    );
}

#[test]
fn space_groups_section_source_and_object_groupings_follow_the_owners() {
    let elf = elf_of(
        vec![
            text(ROM, 0x100),
            Part {
                name: ".rodata",
                flags: 2,
                ..data(ROM + 0x100, 0x20)
            },
        ],
        Some(vec![
            file("lamp.c"),
            local(func("Lamp_Read", ROM, 0x40, 0)),
            file("Com_Main.o"),
            local(func("Com_Main", ROM + 0x40, 0x20, 0)),
        ]),
        vec![(0, LOAD_RX)],
    );
    let sections = flash_groups(&elf, Grouping::Section, &[]);
    assert_eq!(pairs(&sections), [(".text", 0x100), (".rodata", 0x20)]);
    let sources = flash_groups(&elf, Grouping::SourceFile, &[]);
    assert_eq!(pairs(&sources), [(OTHER, 0xC0 + 0x20), ("lamp.c", 0x40)]);
    let objects = flash_groups(&elf, Grouping::ObjectFile, &[]);
    assert_eq!(pairs(&objects)[1], ("Com_Main.o", 0x20));
    assert_eq!(sections[0].by_kind[Kind::Code.index()], 0x100);
    assert_eq!(sections[1].by_kind[Kind::Rodata.index()], 0x20);
}

#[test]
fn space_groups_children_are_the_owners_of_an_elf_group() {
    let elf = module_elf();
    let groups = flash_groups(&elf, Grouping::Module, &[]);
    let rte = groups
        .iter()
        .find(|group| group.name == "Rte_Lamp")
        .unwrap();
    let kids: Vec<_> = rte
        .children
        .iter()
        .map(|child| (child.name.as_str(), child.bytes))
        .collect();
    assert_eq!(kids, [("Rte_Lamp_Read", 0x20), ("Rte_Lamp_Write", 0x20)]);
}

const MAP: &str = "* Link Result
| [in] | | | [out] | | |
| File | Section | Size | Offset | Section | Size |
| obj/Com_Main.o | .text.Com_Main (1) | 0x40 | 0x0 | .text (9) | 0xA0 |
| libs/lib.a(Com_Rx.o) | .text.Com_Rx (2) | 0x30 | 0x40 | .text (9) | 0xA0 |
| obj/Rte_Lamp.o | .text.Rte_Lamp_Read (3) | 0x20 | 0x70 | .text (9) | 0xA0 |
| obj/Lamp_Read.o | .text.Lamp_Read (4) | 0x10 | 0x90 | .text (9) | 0xA0 |
| obj/Com_Main.o | .rodata.table (5) | 0x20 | 0x0 | .rodata (10) | 0x30 |
| obj/lamp_state.o | .zdata.state (6) | 0x10 | 0x0 | .zdata (11) | 0x10 |
* Locate Result
* Sections
+ Space chip:core:linear (MAU = 8bit)
+-------------------------+
| Group | Chip | Section | Space addr | Chip addr | Size (MAU) | Alignment |
|=======|======|=========|============|===========|============|===========|
| code | mc0_rom | .text (9) | 0xa004c000 | 0x0 | 0xa0 | 0x4 |
| code | mc0_rom | *fill* | 0xa004c0a0 | 0xa0 | 0x40 | 0x4 |
| code | mc0_rom | .rodata (10) | 0xa004c0e0 | 0xe0 | 0x30 | 0x4 |
| data | mc0_ram | .zdata (11) | 0x70000000 | 0x0 | 0x10 | 0x4 |
* Memory Usage
| Memory | Code | Data | Reserved | Free | Total |
| mc0_rom | 208 | 0 | 0 | 816 | 1024 |
| mc0_ram | 0 | 16 | 0 | 1008 | 1024 |
";

fn map_groups(map: &MapModel, region: &str, grouping: Grouping) -> Vec<Group> {
    let found = owners_of_map(map, None);
    let regions = regions_for_map(map, None, None);
    group(
        &found,
        Origin::Map(map),
        region_named(&regions, region),
        &by(grouping),
    )
}

#[test]
fn space_groups_map_owners_come_from_located_sections_with_objects() {
    let map = map_of(MAP);
    assert_eq!(map.contributions.len(), 6);
    let found = owners_of_map(&map, None);
    let regions = regions_for_map(&map, None, None);
    for region in &regions.items {
        let inside = found.in_span(region.span().unwrap());
        let total: u64 = inside.iter().map(|owner| owner.size).sum();
        assert_eq!(total, region.used, "{}", region.name);
    }
    let rom = region_named(&regions, "mc0_rom");
    assert_eq!(rom.by_kind[Kind::Code.index()], 0xA0);
    assert_eq!(rom.by_kind[Kind::Rodata.index()], 0x30);
    let text = found.in_span(ROM..ROM + 0xA0);
    let objects: Vec<_> = text
        .iter()
        .map(|owner| (found.object(owner), owner.size, owner.kind))
        .collect();
    assert_eq!(
        objects,
        [
            (Some("obj/Com_Main.o"), 0x40, Kind::Code),
            (Some("libs/lib.a(Com_Rx.o)"), 0x30, Kind::Code),
            (Some("obj/Rte_Lamp.o"), 0x20, Kind::Code),
            (Some("obj/Lamp_Read.o"), 0x10, Kind::Code),
        ]
    );
    assert_eq!(found.provenance.from_map, 6);
    assert_eq!(found.provenance.not_attributed, 1);
    assert!(found.items.iter().all(|owner| owner.kind != Kind::Fill));
}

#[test]
fn space_groups_map_modules_take_object_stems_and_list_object_children() {
    let map = map_of(MAP);
    let groups = map_groups(&map, "mc0_rom", Grouping::Module);
    assert_eq!(
        pairs(&groups),
        [
            ("Com", 0x40 + 0x30 + 0x20),
            ("Rte_Lamp", 0x20),
            (NOT_ATTRIBUTED, 0x10),
            ("Lamp", 0x10)
        ]
    );
    let com = &groups[0];
    let kids: Vec<_> = com
        .children
        .iter()
        .map(|child| (child.name.as_str(), child.bytes))
        .collect();
    assert_eq!(
        kids,
        [
            ("obj/Com_Main.o", 0x40 + 0x20),
            ("libs/lib.a(Com_Rx.o)", 0x30)
        ]
    );
    assert_eq!(com.by_kind[Kind::Code.index()], 0x70);
    assert_eq!(com.by_kind[Kind::Rodata.index()], 0x20);
}

#[test]
fn space_groups_map_object_and_section_groupings() {
    let map = map_of(MAP);
    let objects = map_groups(&map, "mc0_rom", Grouping::ObjectFile);
    assert_eq!(objects[0].name, "obj/Com_Main.o");
    assert_eq!(objects[0].bytes, 0x60);
    let sections = map_groups(&map, "mc0_rom", Grouping::Section);
    assert_eq!(pairs(&sections), [(".text", 0xA0), (".rodata", 0x30)]);
    let ram = map_groups(&map, "mc0_ram", Grouping::Module);
    assert_eq!(pairs(&ram), [("lamp", 0x10)]);
}

#[test]
fn space_groups_fill_never_forms_a_group() {
    let map = map_of(MAP);
    let regions = regions_for_map(&map, None, None);
    let rom = region_named(&regions, "mc0_rom");
    assert_eq!(rom.by_kind[Kind::Fill.index()], 0x40);
    for grouping in Grouping::ALL {
        let groups = map_groups(&map, "mc0_rom", grouping);
        let total: u64 = groups.iter().map(|group| group.bytes).sum();
        assert_eq!(total, rom.used, "{grouping:?}");
        assert!(
            groups
                .iter()
                .all(|group| group.by_kind[Kind::Fill.index()] == 0)
        );
    }

    let fill = |address, len| Part {
        name: "$fill",
        flags: 2,
        ..data(address, len)
    };
    let elf = elf_of(
        vec![text(ROM, 0x100), fill(ROM + 0x100, 0x80)],
        Some(vec![func("Com_Main", ROM, 0x100, 0)]),
        vec![(0, LOAD_RX)],
    );
    let groups = flash_groups(&elf, Grouping::Section, &[]);
    assert_eq!(pairs(&groups), [(".text", 0x100)]);
}

#[test]
fn space_groups_availability_follows_the_data() {
    let map = map_of(MAP);
    let found = owners_of_map(&map, None);
    assert_eq!(
        available(&found, ROM..ROM + 0x400),
        [true, false, true, true]
    );

    let stripped = elf_of(vec![text(ROM, 0x40)], None, vec![(0, LOAD_RX)]);
    let found = owners(&stripped, None, None);
    assert_eq!(
        available(&found, ROM..ROM + 0x40),
        [true, false, false, true]
    );
    assert_eq!(available(&found, DSPR..DSPR + 0x40), [false; 4]);
}

#[test]
fn space_groups_default_takes_the_first_grouping_with_more_than_one_group() {
    let elf = module_elf();
    let found = owners(&elf, None, None);
    let regions = regions_for_elf(&elf, Some(&table()));
    let rom = region_named(&regions, "pflash");
    let origin = Origin::Elf {
        elf: &elf,
        map: None,
    };
    assert_eq!(default_grouping(&found, origin, rom, &[]), Grouping::Module);

    let one_module = map_of(
        "* Link Result\n| File | Section | Size | Offset | Section | Size |\n\
         | Com_A.o | .text.a (1) | 0x10 | 0x0 | .text (9) | 0x30 |\n\
         | Com_B.o | .text.b (2) | 0x20 | 0x10 | .text (9) | 0x30 |\n\
         * Locate Result\n* Sections\n+ Space chip:core:linear (MAU = 8bit)\n\
         | Group | Chip | Section | Space addr | Chip addr | Size (MAU) | Alignment |\n\
         | code | mc0_rom | .text (9) | 0xa004c000 | 0x0 | 0x30 | 0x4 |\n\
         * Memory Usage\n| Memory | Code | Data | Reserved | Free | Total |\n\
         | mc0_rom | 48 | 0 | 0 | 976 | 1024 |\n",
    );
    let found = owners_of_map(&one_module, None);
    let regions = regions_for_map(&one_module, None, None);
    let origin = Origin::Map(&one_module);
    let rom = &regions.items[0];
    assert_eq!(
        default_grouping(&found, origin, rom, &[]),
        Grouping::ObjectFile
    );
}

#[test]
fn space_groups_default_for_a_stripped_elf_is_section() {
    let elf = elf_of(
        vec![
            text(ROM, 0x40),
            Part {
                name: ".rodata",
                flags: 2,
                ..data(ROM + 0x40, 0x20)
            },
        ],
        None,
        vec![(0, LOAD_RX)],
    );
    let found = owners(&elf, None, None);
    let regions = regions_for_elf(&elf, Some(&table()));
    let rom = region_named(&regions, "pflash");
    let origin = Origin::Elf {
        elf: &elf,
        map: None,
    };
    assert_eq!(
        default_grouping(&found, origin, rom, &[]),
        Grouping::Section
    );
}

#[test]
fn space_groups_flash_region_is_the_one_with_the_most_code_and_rodata() {
    let elf = elf_of(
        vec![
            text(ROM, 0x100),
            Part {
                load_address: ROM + 0x100,
                ..data(DSPR, 0x400)
            },
        ],
        Some(vec![func("main", ROM, 0x100, 0)]),
        vec![(0, LOAD_RX), (1, LOAD_RW)],
    );
    let with_mdf = regions_for_elf(&elf, Some(&table()));
    let at = flash_region(&with_mdf).unwrap();
    assert_eq!(with_mdf.items[at].name, "pflash");

    let clusters = regions_for_elf(&elf, None);
    let at = flash_region(&clusters).unwrap();
    assert_eq!(clusters.items[at].start, Some(ROM));
    assert_eq!(flash_region(&Regions::default()), None);

    let ram_only = elf_of(vec![bss(DSPR, 0x40)], None, vec![(0, LOAD_RW)]);
    let regions = regions_for_elf(&ram_only, Some(&table()));
    let at = flash_region(&regions).unwrap();
    assert_eq!(regions.items[at].name, "dspr");
}

#[test]
fn space_groups_a_rom_chip_beats_a_bigger_ram_region() {
    let elf = elf_of(
        vec![
            text(DSPR, 0x400),
            Part {
                name: ".text",
                ..text(ROM, 0x40)
            },
        ],
        None,
        Vec::new(),
    );
    let regions = regions_for_elf(&elf, Some(&table()));
    let at = flash_region(&regions).unwrap();
    assert_eq!(regions.items[at].name, "pflash");
}

#[test]
fn space_groups_flash_groups_count_load_images_and_mark_them() {
    let init = Part {
        load_address: ROM + 0x100,
        ..data(DSPR, 0x40)
    };
    let elf = elf_of(
        vec![text(ROM, 0x100), init],
        Some(vec![
            func("Com_Main", ROM, 0x100, 0),
            object("Com_counter", DSPR, 0x40, 1),
        ]),
        vec![(0, LOAD_RX), (1, LOAD_RW)],
    );
    let groups = flash_groups(&elf, Grouping::Module, &[]);
    assert_eq!(pairs(&groups), [("Com", 0x140)]);
    let regions = regions_for_elf(&elf, Some(&table()));
    assert_eq!(region_named(&regions, "pflash").used, 0x140);
    assert_eq!(groups[0].image, 0x40);
    assert_eq!(groups[0].by_kind[Kind::Data.index()], 0x40);

    let found = owners(&elf, None, None);
    let regions = regions_for_elf(&elf, Some(&table()));
    let ram = region_named(&regions, "dspr");
    let origin = Origin::Elf {
        elf: &elf,
        map: None,
    };
    let in_ram = group(&found, origin, ram, &by(Grouping::Module));
    assert_eq!(pairs(&in_ram), [("Com", 0x40)]);
    assert_eq!(in_ram[0].image, 0);
}

#[test]
fn space_groups_cluster_regions_without_a_total_still_group() {
    let elf = module_elf();
    let found = owners(&elf, None, None);
    let regions = regions_for_elf(&elf, None);
    let origin = Origin::Elf {
        elf: &elf,
        map: None,
    };
    let groups = group(&found, origin, &regions.items[0], &by(Grouping::Module));
    let total: u64 = groups.iter().map(|group| group.bytes).sum();
    assert_eq!(total, regions.items[0].used);
}

#[test]
fn space_groups_top_keeps_22_and_counts_the_rest() {
    let names: Vec<String> = (0..25).map(|n| format!("M{n:02}")).collect();
    let rows: String = names
        .iter()
        .enumerate()
        .map(|(n, name)| {
            let offset = n * 0x10;
            format!(
                "| {name}_x.o | .text.f{n} (1) | 0x{size:x} | 0x{offset:x} | .text (9) | 0x190 |\n",
                size = 0x10
            )
        })
        .collect();
    let body = format!(
        "* Link Result\n| File | Section | Size | Offset | Section | Size |\n{rows}\
         * Locate Result\n* Sections\n+ Space chip:core:linear (MAU = 8bit)\n\
         | Group | Chip | Section | Space addr | Chip addr | Size (MAU) | Alignment |\n\
         | code | mc0_rom | .text (9) | 0xa004c000 | 0x0 | 0x190 | 0x4 |\n\
         * Memory Usage\n| Memory | Code | Data | Reserved | Free | Total |\n\
         | mc0_rom | 400 | 0 | 0 | 624 | 1024 |\n"
    );
    let map = map_of(&body);
    let groups = map_groups(&map, "mc0_rom", Grouping::Module);
    assert_eq!(groups.len(), 25);
    let cut = top(&groups, TOP_GROUPS);
    assert_eq!(cut.shown.len(), 22);
    assert_eq!((cut.more, cut.more_bytes), (3, 0x30));
    let all = top(&groups[..3], TOP_GROUPS);
    assert_eq!((all.shown.len(), all.more, all.more_bytes), (3, 0, 0));
}

const COPY: &str = "[.data.Com.State]";

#[test]
fn space_groups_an_elf_rom_copy_belongs_to_its_module_as_rodata() {
    let copy = Part {
        name: COPY,
        flags: 2,
        ..data(ROM + 0x40, 0x20)
    };
    let elf = elf_of(vec![text(ROM, 0x40), copy], None, vec![(0, LOAD_RX)]);
    let regions = regions_for_elf(&elf, Some(&table()));
    let flash = region_named(&regions, "pflash");
    assert_eq!(flash.by_kind[Kind::Rodata.index()], 0x20);
    let found = owners(&elf, None, None);
    let origin = Origin::Elf {
        elf: &elf,
        map: None,
    };
    let groups = group(&found, origin, flash, &by(Grouping::Module));
    let com = groups.iter().find(|group| group.name == "Com").unwrap();
    assert_eq!((com.bytes, com.by_kind[Kind::Rodata.index()]), (0x20, 0x20));
}

#[test]
fn space_groups_a_map_rom_copy_belongs_to_its_module_as_rodata() {
    let map = map_of(
        "* Link Result\n| File | Section | Size | Offset | Section | Size |\n\
         | _.o | [.data.Com.State] (1) | 0x10 | 0x0 | [.data.Com.State] (9) | 0x10 |\n\
         * Locate Result\n* Sections\n+ Space chip:core:linear (MAU = 8bit)\n\
         | Group | Chip | Section | Space addr | Chip addr | Size (MAU) | Alignment |\n\
         | code | mc0_rom | [.data.Com.State] (9) | 0xa004c000 | 0x0 | 0x10 | 0x4 |\n\
         * Memory Usage\n| Memory | Code | Data | Reserved | Free | Total |\n\
         | mc0_rom | 16 | 0 | 0 | 1008 | 1024 |\n",
    );
    let regions = regions_for_map(&map, None, None);
    assert_eq!(regions.items[0].by_kind[Kind::Rodata.index()], 0x10);
    let found = owners_of_map(&map, None);
    let groups = group(
        &found,
        Origin::Map(&map),
        &regions.items[0],
        &by(Grouping::Module),
    );
    assert_eq!(pairs(&groups), [("Com", 0x10)]);
    assert_eq!(groups[0].by_kind[Kind::Rodata.index()], 0x10);
}

#[test]
fn space_groups_overlapping_regions_do_not_count_owners_twice() {
    let mdf = parse_mdf(
        "space a 1 8\nspace b 2 8\nchip x 10 8 64 ram\nchip y 11 8 64 ram\n\
         map 1 0x100 64 10 0\nmap 2 0x100 64 11 0\n",
    )
    .regions();
    let elf = elf_of(
        vec![data(0x100, 0x20)],
        Some(vec![object("Com_state", 0x100, 0x20, 0)]),
        vec![(0, LOAD_RW)],
    );
    let regions = regions_for_elf(&elf, Some(&mdf));
    let found = owners(&elf, None, None);
    let origin = Origin::Elf {
        elf: &elf,
        map: None,
    };
    let first = group(&found, origin, &regions.items[0], &by(Grouping::Module));
    assert_eq!(pairs(&first), [("Com", 0x20)]);
    let second = &regions.items[1];
    assert_eq!(second.used, 0);
    assert!(group(&found, origin, second, &by(Grouping::Module)).is_empty());
}

#[test]
fn space_groups_symbols_in_a_named_section_group_by_their_own_names() {
    let elf = elf_of(
        vec![Part {
            name: ".text.cpu0_code",
            ..text(ROM, 0x40)
        }],
        Some(vec![
            func("Com_Main", ROM, 0x20, 0),
            func("Lamp_Run", ROM + 0x20, 0x20, 0),
        ]),
        vec![(0, LOAD_RX)],
    );
    let groups = flash_groups(&elf, Grouping::Module, &[]);
    assert_eq!(pairs(&groups), [("Com", 0x20), ("Lamp", 0x20)]);
}

#[test]
fn space_groups_children_carry_their_own_kinds() {
    let elf = module_elf();
    let groups = flash_groups(&elf, Grouping::Module, &[]);
    for group in &groups {
        for child in &group.children {
            assert_eq!(child.by_kind.iter().sum::<u64>(), child.bytes);
        }
        let from_children: u64 = group
            .children
            .iter()
            .map(|c| c.by_kind[Kind::Code.index()])
            .sum();
        assert_eq!(from_children, group.by_kind[Kind::Code.index()]);
    }
    let rte = groups.iter().find(|g| g.name == "Rte_Lamp").unwrap();
    assert!(
        rte.children
            .iter()
            .all(|child| child.by_kind[Kind::Code.index()] == child.bytes)
    );
}
