use std::sync::atomic::AtomicBool;

use rusty_automotive::map::{MapFlavor, MapModel, MapProblem, index_map};

use super::facts::{Inspection, inspect};
use super::loaded::Loaded;
use super::scope::Scope;
use super::select::{Column, Query, Table, select};
use super::text::{INCONSISTENT, LOSSY, MAU, address, percent, span, status};
use super::{MapBody, MapView};
use crate::window_view::FerriteWindow;
use gpui_kit::{Entity, TestAppContext};

pub(in crate::window_view::viewer) const SAMPLE: &str = "\
TASKING VX-toolset for TriCore linker map file
* Tool and Invocation
invented invocation
* Link Result
| [in] | | | [out] | | |
| File | Section | Size | Offset | Section | Size |
| obj/engine.o | .text.start (1) | 0x100 | 0 | .text.boot (1) | 0x180 |
|| .text.run (2) | 0x80 | 0x100 | .text.boot (1) | 0x180 |
|| .data.tune (3) | 0x40 | 0 | .data.cal (3) | 0x40 |
| obj/lunar.o | .text.turn (4) | 0x200 | 0 | .text.app (2) | 0x200 |
| obj/comet.o | .bss.buf (5) | 0x400 | 0 | .bss.ram (4) | 0x400 |
* Locate Result
* Sections
+ Space mpe:vtc:linear (MAU = 8bit)
+------+
| Group | Chip | Section | Space addr | Chip addr | Size (MAU) | Alignment |
|=======|======|=========|============|===========|============|===========|
| code | mpe:flash0 | .text.boot (1) | 0x80000000 | 0x0 | 0x180 | 0x4 |
| code | mpe:flash0 | .text.app (2) | 0x80000200 | 0x200 | 0x200 | 0x4 |
| data | mpe:flash0 | .data.cal (3) | 0x80001000 | 0x1000 | 0x40 | 0x4 |
| data | mpe:dsram0 | .bss.ram (4) | 0x70000000 | 0x0 | 0x400 | 0x4 |
* Memory Usage
| Memory | Code | Data | Reserved | Free | Total |
| mpe:flash0 | 896 | 64 | 0 | 64576 | 65536 |
| mpe:dsram0 | 0 | 1024 | 0 | 31744 | 32768 |
| mpe:pflash1 | 0 | 0 | 0 | 4096 | 4096 |
* Symbols (sorted on name)
| Name | Address | Size | Space |
| engine_init | 0x80000000 | 0x100 | mpe:vtc:linear |
| engine_run | 0x80000100 | 0x80 | mpe:vtc:linear |
| turn_left | 0x80000200 | 0x100 | mpe:vtc:linear |
| turn_right | 0x80000300 | 0x100 | mpe:vtc:linear |
| tune_table | 0x80001000 | 0x40 | mpe:vtc:linear |
| rx_buffer | 0x70000000 | 0x400 | mpe:vtc:linear |
* Removed Sections
| Section | File | Library | Symbol | Reason |
| .text.old (7) | obj/old.o | old.a | old_turn | unreferenced |
";

pub(super) fn loaded_from(text: &str) -> Loaded {
    let model = index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {})
        .expect("the sample is a map");
    Loaded::new(model)
}

fn loaded() -> Loaded {
    loaded_from(SAMPLE)
}

fn names(loaded: &Loaded, query: &Query) -> Vec<String> {
    let rows = select(loaded, query, &AtomicBool::new(false)).expect("not cancelled");
    let model = &loaded.model;
    rows.into_iter()
        .map(|at| match query.table {
            Table::Memory => model.regions[at as usize].name.clone(),
            Table::Sections => model.sections[at as usize].name.clone(),
            Table::Symbols => model.symbols[at as usize].name.clone(),
        })
        .collect()
}

fn query(table: Table) -> Query {
    let mut query = Query::default();
    query.table = table;
    query
}

#[test]
fn addresses_and_spans_read_as_hex_with_the_width_of_the_value() {
    assert_eq!(address(0x8000_0010), "0x80000010");
    assert_eq!(address(0x1_0000_0000), "0x0000000100000000");
    assert_eq!(span(0x100, 0x40), "0x00000100–0x0000013F");
    assert_eq!(span(0x100, 0), "0x00000100");
}

#[test]
fn used_percent_is_text_with_one_decimal_and_a_dash_for_no_length() {
    assert_eq!(percent(1024, 32768), "3.1%");
    assert_eq!(percent(0, 4096), "0.0%");
    assert_eq!(percent(7, 0), "–");
}

#[test]
fn memory_starts_with_the_fullest_region_and_sorts_by_each_column() {
    let loaded = loaded();
    let mut query = query(Table::Memory);
    assert_eq!(
        names(&loaded, &query),
        ["mpe:dsram0", "mpe:flash0", "mpe:pflash1"]
    );
    query.sort_by(Column::Name);
    assert_eq!(
        names(&loaded, &query),
        ["mpe:dsram0", "mpe:flash0", "mpe:pflash1"]
    );
    query.sort_by(Column::Size);
    assert_eq!(names(&loaded, &query)[0], "mpe:pflash1");
    query.sort_by(Column::Size);
    assert_eq!(names(&loaded, &query)[0], "mpe:flash0");
}

#[test]
fn sorting_the_same_column_again_reverses_it_and_each_table_keeps_its_order() {
    let loaded = loaded();
    let mut query = query(Table::Symbols);
    assert_eq!(names(&loaded, &query)[0], "rx_buffer");
    query.sort_by(Column::Name);
    assert_eq!(names(&loaded, &query)[0], "engine_init");
    query.sort_by(Column::Name);
    assert_eq!(names(&loaded, &query)[0], "turn_right");
    query.table = Table::Sections;
    assert_eq!(names(&loaded, &query)[0], ".bss.ram (4)");
    query.table = Table::Symbols;
    assert_eq!(names(&loaded, &query)[0], "turn_right");
}

#[test]
fn typing_matches_names_in_any_case() {
    let loaded = loaded();
    let mut query = query(Table::Symbols);
    query.text = "TURN".to_owned();
    assert_eq!(names(&loaded, &query), ["turn_left", "turn_right"]);
    query.table = Table::Sections;
    query.text = ".text".to_owned();
    assert_eq!(names(&loaded, &query), [".text.boot (1)", ".text.app (2)"]);
    query.text = "no such name".to_owned();
    assert!(names(&loaded, &query).is_empty());
}

#[test]
fn a_region_scope_keeps_its_sections_and_the_symbols_inside_them() {
    let loaded = loaded();
    let mut query = query(Table::Sections);
    query.scope = Scope::Region("mpe:flash0".to_owned());
    assert_eq!(
        names(&loaded, &query),
        [".text.boot (1)", ".text.app (2)", ".data.cal (3)"]
    );
    query.table = Table::Symbols;
    assert_eq!(
        names(&loaded, &query),
        [
            "engine_init",
            "engine_run",
            "turn_left",
            "turn_right",
            "tune_table"
        ]
    );
    query.table = Table::Memory;
    assert_eq!(names(&loaded, &query), ["mpe:flash0"]);
}

#[test]
fn an_object_scope_keeps_what_it_contributes() {
    let loaded = loaded();
    let mut query = query(Table::Sections);
    query.scope = Scope::Object("obj/engine.o".to_owned());
    assert_eq!(names(&loaded, &query), [".text.boot (1)", ".data.cal (3)"]);
    query.table = Table::Symbols;
    assert_eq!(
        names(&loaded, &query),
        ["engine_init", "engine_run", "tune_table"]
    );
    query.table = Table::Memory;
    assert_eq!(names(&loaded, &query), ["mpe:flash0"]);
    query.scope = Scope::Object("obj/comet.o".to_owned());
    assert_eq!(names(&loaded, &query), ["mpe:dsram0"]);
}

#[test]
fn a_cancelled_select_returns_nothing() {
    let loaded = loaded();
    let cancel = AtomicBool::new(true);
    assert!(select(&loaded, &Query::default(), &cancel).is_none());
}

#[test]
fn objects_merge_by_file_and_the_largest_comes_first() {
    let loaded = loaded();
    let objects: Vec<_> = loaded
        .objects
        .iter()
        .map(|object| (object.name.as_str(), object.bytes))
        .collect();
    assert_eq!(
        objects,
        [
            ("obj/comet.o", 1024),
            ("obj/lunar.o", 512),
            ("obj/engine.o", 448)
        ]
    );
    assert_eq!(loaded.objects[2].regions, ["mpe:flash0"]);
}

#[test]
fn a_region_inspection_has_totals_and_its_top_objects() {
    let loaded = loaded();
    let at = loaded
        .model
        .regions
        .iter()
        .position(|region| region.name == "mpe:flash0")
        .expect("flash0");
    let Some(Inspection::Region { region, objects }) = inspect(&loaded, Table::Memory, at) else {
        panic!("a region");
    };
    assert_eq!(
        (region.length, region.used, region.free),
        (65536, 960, 64576)
    );
    assert_eq!(
        objects,
        [
            ("obj/lunar.o".to_owned(), 512),
            ("obj/engine.o".to_owned(), 448)
        ]
    );
}

#[test]
fn a_section_inspection_lists_its_contributions_largest_first() {
    let loaded = loaded();
    let at = loaded
        .model
        .sections
        .iter()
        .position(|section| section.name == ".text.boot (1)")
        .expect("boot");
    let Some(Inspection::Section {
        section,
        parts,
        more,
    }) = inspect(&loaded, Table::Sections, at)
    else {
        panic!("a section");
    };
    assert_eq!((section.address, section.size), (0x8000_0000, 0x180));
    assert_eq!(more, 0);
    let listed: Vec<_> = parts
        .iter()
        .map(|part| (part.object.as_str(), part.input.as_str(), part.size))
        .collect();
    assert_eq!(
        listed,
        [
            ("obj/engine.o", ".text.start (1)", 0x100),
            ("obj/engine.o", ".text.run (2)", 0x80)
        ]
    );
}

#[test]
fn a_symbol_inspection_finds_its_section_and_object_by_address() {
    let loaded = loaded();
    let at = loaded
        .model
        .symbols
        .iter()
        .position(|symbol| symbol.name == "engine_run")
        .expect("engine_run");
    let Some(Inspection::Symbol {
        symbol,
        section,
        held_by,
    }) = inspect(&loaded, Table::Symbols, at)
    else {
        panic!("a symbol");
    };
    assert_eq!((symbol.address, symbol.size), (0x8000_0100, Some(0x80)));
    assert_eq!(section.as_deref(), Some(".text.boot (1)"));
    let part = held_by.expect("held by an object");
    assert_eq!(
        (part.object.as_str(), part.input.as_str()),
        ("obj/engine.o", ".text.run (2)")
    );
}

fn texts(loaded: &Loaded) -> Vec<String> {
    status(loaded).into_iter().map(|check| check.text).collect()
}

#[test]
fn the_status_names_the_toolchain_counts_totals_and_skipped_parts() {
    assert_eq!(
        texts(&loaded()),
        [
            "TASKING",
            "3 regions",
            "4 sections",
            "6 symbols",
            "used 1.9 KB · free 98.1 KB",
            "Not read: Tool and Invocation",
        ]
    );
}

#[test]
fn each_kind_of_problem_has_its_own_status_entry() {
    let mut text = SAMPLE.replace(
        "| mpe:pflash1 | 0 | 0 | 0 | 4096 | 4096 |",
        "| mpe:pflash1 | 9 | 0 | 0 | 4096 | 4096 |",
    );
    text.push_str("+ Space mpe:wide (MAU = 16bit)\n| Name | Address |\n| bad | 0x1 |\n");
    text.push_str(
        "* Symbols (sorted on name)\n| Name | Address | Size | Space |\n| broken | zz | 1 | x |\n",
    );
    let mut bytes = text.into_bytes();
    bytes.extend_from_slice(b"* Removed Sections\n| Section | File | Library | Symbol | Reason |\n| \xff.bad | a.o | | | r |\n");
    let model = index_map(bytes.as_slice(), &AtomicBool::new(false), &mut |_| {}).expect("map");
    let all = texts(&Loaded::new(model));
    assert_eq!(
        all[..4],
        [
            "some text was not valid UTF-8 and was decoded lossily",
            "1 address space with a unit other than 8 bit skipped",
            "1 region where code + data + reserved differ from used",
            "1 row could not be read (line 43: missing column, invalid number or overflowing range)",
        ]
    );
}

#[test]
fn problem_reasons_match_the_ones_the_reader_reports() {
    let text = format!("{SAMPLE}+ Space mpe:wide (MAU = 16bit)\n");
    let model = index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {}).expect("map");
    let reasons: Vec<_> = model
        .problems
        .iter()
        .filter_map(|problem| match problem {
            MapProblem::Malformed { reason, .. } => Some(*reason),
            MapProblem::CapReached { .. } => None,
        })
        .collect();
    assert_eq!(reasons, [MAU]);
    assert!(![LOSSY, INCONSISTENT].contains(&MAU));
}

#[test]
fn caps_and_unlisted_problems_are_named_in_the_status() {
    let mut model: MapModel = loaded().model;
    model.flavor = MapFlavor::Tasking;
    model.problems = vec![MapProblem::CapReached {
        line: 7,
        limit: "retained entries",
    }];
    model.problem_total = 1001;
    let texts = texts(&Loaded::new(model));
    assert!(
        texts.contains(
            &"limit reached at line 7 (retained entries): the lists are incomplete".to_owned()
        ),
        "{texts:?}"
    );
    assert!(
        texts.contains(&"1,000 rows could not be read".to_owned()),
        "{texts:?}"
    );
}

fn head_names(loaded: &Loaded) -> Vec<&'static str> {
    super::text::heads(loaded, Table::Memory)
        .iter()
        .map(|head| head.name)
        .collect()
}

#[test]
fn the_origin_column_shows_only_when_the_map_gives_an_origin() {
    let plain = loaded();
    assert_eq!(
        head_names(&plain),
        ["Region", "Length", "Used", "Free", "Used %"]
    );
    assert_eq!(
        super::text::cells(&plain, Table::Memory, 0),
        ["mpe:flash0", "65,536", "960", "64,576", "1.4%"]
    );
    let with_origin = SAMPLE.replace(
        "| Memory | Code | Data | Reserved | Free | Total |\n| mpe:flash0 | 896 | 64 | 0 | 64576 | 65536 |",
        "| Memory | Origin | Code | Data | Reserved | Free | Total |\n| mpe:flash0 | 0x80000000 | 896 | 64 | 0 | 64576 | 65536 |",
    );
    let given = loaded_from(&with_origin);
    assert_eq!(
        head_names(&given),
        ["Region", "Origin", "Length", "Used", "Free", "Used %"]
    );
    assert_eq!(
        super::text::cells(&given, Table::Memory, 0),
        [
            "mpe:flash0",
            "0x80000000",
            "65,536",
            "960",
            "64,576",
            "1.4%"
        ]
    );
}

pub(in crate::window_view::viewer) struct Probe {
    pub nav_cursor: usize,
    pub has_inspection: bool,
}

fn map_view(cx: &mut TestAppContext, window: &Entity<FerriteWindow>) -> Entity<MapView> {
    window.read_with(cx, |window, _| {
        let body = window
            .viewer
            .as_ref()
            .and_then(|viewer| viewer.body.as_ref());
        body.and_then(|body| body.as_any().downcast_ref::<MapBody>())
            .expect("a map body")
            .0
            .clone()
    })
}

pub(in crate::window_view::viewer) fn probe(
    cx: &mut TestAppContext,
    window: &Entity<FerriteWindow>,
) -> Probe {
    let map = map_view(cx, window);
    map.read_with(cx, |map, _| Probe {
        nav_cursor: map.nav_cursor,
        has_inspection: map.inspection.is_some(),
    })
}

pub(in crate::window_view::viewer) fn sort_by_size_then_finish_parsing(
    cx: &mut TestAppContext,
    window: &Entity<FerriteWindow>,
) {
    let map = map_view(cx, window);
    map.update(cx, |map, cx| {
        let loaded = map.loaded().expect("loaded").clone();
        map.query.sort_by(Column::Size);
        let rows = select(&loaded, &Query::default(), &AtomicBool::new(false)).expect("rows");
        map.index_message(super::jobs::IndexMessage::Parsed(Ok((loaded, rows))), cx);
    });
}

pub(in crate::window_view::viewer) fn column_width(
    cx: &mut TestAppContext,
    window: &Entity<FerriteWindow>,
    table: usize,
    at: usize,
) -> gpui_kit::Pixels {
    let map = map_view(cx, window);
    map.read_with(cx, |map, cx| {
        let columns = map.columns.as_ref().expect("columns");
        columns.set(Table::ALL[table]).read(cx).width(at)
    })
}

#[test]
fn a_long_section_name_keeps_its_module_and_a_symbol_its_prefix() {
    let section = super::columns::fitted_name(
        Table::Sections,
        ".text.Com.Com_SendSignalGroupArrayWithTimeout",
        24,
    );
    assert_eq!(section.prefix, "Com.");
    assert!(section.rest.contains('…'));
    let symbol = super::columns::fitted_name(
        Table::Symbols,
        "Can_MainFunction_ReadAllChannelsAndReport",
        24,
    );
    assert_eq!(symbol.prefix, "Can_");
    assert!(symbol.rest.starts_with("Main") && symbol.rest.ends_with("Report"));
    let region = super::columns::fitted_name(Table::Memory, "mpe:flash0", 24);
    assert_eq!(
        (region.prefix.as_str(), region.rest.as_str()),
        ("", "mpe:flash0")
    );
}
