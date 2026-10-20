#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "synthetic release benchmarks assert inputs and print their budgets"
)]

mod elf_build;
#[path = "elf_build/rich.rs"]
mod rich;

use std::{
    io::Cursor,
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

use rusty_automotive::{
    Class, Elf, ElfReader, Endian, Lines, MapModel, Query, Watch, index_map, parse_mdf, select,
    space::{
        self, Group, GroupBy, Grouping, Item, Kind, Origin, Owners, Part, Rect, Regions, Scale,
        ScaleMode,
    },
};

use rich::{MDF, estimated_layout, merged_layout, split_layout};

#[derive(Default)]
struct Timings(Vec<(String, Duration, Option<u64>)>);

impl Timings {
    fn time<T>(&mut self, name: impl Into<String>, run: impl FnOnce() -> T) -> (T, Duration) {
        let started = Instant::now();
        let value = std::hint::black_box(run());
        let took = started.elapsed();
        self.0.push((name.into(), took, None));
        (value, took)
    }

    fn budget(&mut self, name: impl Into<String>, took: Duration, millis: u64) {
        self.0.push((name.into(), took, Some(millis)));
    }

    fn limit_last(&mut self, millis: u64) {
        self.0.last_mut().unwrap().2 = Some(millis);
    }

    fn finish(self) {
        println!("| metric | ms | budget ms |\n| --- | ---: | ---: |");
        for (name, took, budget) in &self.0 {
            println!(
                "| {name} | {:.3} | {} |",
                took.as_secs_f64() * 1000.0,
                budget.map_or_else(|| "—".into(), |limit| limit.to_string())
            );
        }
        #[cfg(target_os = "linux")]
        {
            let path = std::path::Path::new(std::path::MAIN_SEPARATOR_STR).join("proc/self/status");
            let status = std::fs::read_to_string(path).unwrap();
            let peak: u64 = status
                .lines()
                .find_map(|line| {
                    line.strip_prefix("VmHWM:")?
                        .split_whitespace()
                        .next()?
                        .parse()
                        .ok()
                })
                .unwrap();
            println!(
                "| peak RSS (MB, including fixture generation) | {:.3} | 200 |",
                peak as f64 * 1024.0 / 1_000_000.0
            );
            #[cfg(not(debug_assertions))]
            assert!(peak * 1024 <= 200_000_000, "peak RSS: {peak} KiB");
        }
        #[cfg(not(debug_assertions))]
        for (name, took, budget) in &self.0 {
            if let Some(limit) = budget {
                assert!(
                    *took <= Duration::from_millis(*limit),
                    "{name}: {took:?} > {limit} ms"
                );
            }
        }
    }
}

struct Model<'a> {
    owners: Owners,
    regions: Regions,
    origin: Origin<'a>,
    elapsed: Duration,
}

fn items(groups: &[Group]) -> Vec<Item> {
    groups
        .iter()
        .enumerate()
        .map(|(id, group)| Item {
            id: id as u32,
            value: group.bytes,
            parts: group
                .children
                .iter()
                .enumerate()
                .map(|(id, child)| Part {
                    id: id as u32,
                    value: child.bytes,
                })
                .collect(),
        })
        .collect()
}

fn measure_strips(timings: &mut Timings, name: &str, model: &Model<'_>) {
    for region in &model.regions.items {
        let inside = model.owners.in_span(region.span().unwrap());
        let used: u64 = inside.iter().map(|owner| owner.size).sum();
        assert_eq!(used, region.used, "{name}/{} owners", region.name);
        for mode in [ScaleMode::Compressed, ScaleMode::True] {
            let (cells, _) = timings.time(format!("{name}/strip/{}/{mode:?}", region.name), || {
                let scale = Scale::new(region, mode, None);
                space::cells(region, &scale, 1600)
            });
            timings.limit_last(5);
            assert!(!cells.is_empty());
            assert!(cells.len() <= 1600);
        }
    }
}

fn measure_model(timings: &mut Timings, name: &str, mut model: Model<'_>) {
    assert_eq!(model.regions.outside, 0);
    assert_eq!(model.regions.items.len(), 2);
    measure_strips(timings, name, &model);
    for grouping in Grouping::ALL {
        let by = GroupBy {
            grouping,
            modules: &[],
        };
        let (groups, took) = timings.time(format!("{name}/groups/{grouping:?}"), || {
            model
                .regions
                .items
                .iter()
                .map(|region| space::group(&model.owners, model.origin, region, &by))
                .collect::<Vec<_>>()
        });
        model.elapsed += took;
        for (region, groups) in model.regions.items.iter().zip(groups) {
            let images: u64 = model
                .owners
                .in_span(region.span().unwrap())
                .iter()
                .filter(|owner| owner.image)
                .map(|owner| owner.size)
                .sum();
            assert_eq!(
                groups.iter().map(|group| group.bytes).sum::<u64>(),
                region.used
            );
            assert_eq!(groups.iter().map(|group| group.image).sum::<u64>(), images);
            assert!(
                groups
                    .iter()
                    .all(|group| group.by_kind[Kind::Fill.index()] == 0)
            );
            let items = items(&groups);
            let (tiles, _) = timings.time(
                format!("{name}/treemap/{}/{grouping:?}", region.name),
                || space::layout(&items, Rect::new(0.0, 0.0, 1600.0, 900.0)),
            );
            timings.limit_last(1);
            assert_eq!(
                tiles.iter().map(|tile| tile.value).sum::<u64>(),
                region.used
            );
        }
    }
    timings.budget(format!("{name}/Rich model"), model.elapsed, 150);
}

fn parse_elf(bytes: &[u8], timings: &mut Timings) -> (Elf, Lines) {
    assert!(
        (26_000_000..34_000_000).contains(&bytes.len()),
        "{} bytes",
        bytes.len()
    );
    let cancel = AtomicBool::new(false);
    let mut progress = |_| {};
    let mut watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    let reader = ElfReader::new(Cursor::new(bytes));
    let (elf, parse_time) = timings.time("ELF parse", || reader.parse(&mut watch).unwrap());
    let (lines, lines_time) =
        timings.time("DWARF lines", || reader.lines(&elf, &mut watch).unwrap());
    println!(
        "ELF: {} bytes, {} sections, {} LOAD segments, {} indexed symbols",
        bytes.len(),
        elf.sections.len(),
        elf.segments.len(),
        elf.symbols.len()
    );
    timings.budget("parse + DWARF lines", parse_time + lines_time, 300);
    (elf, lines)
}

fn measure_elf(bytes: &[u8], map: Option<&MapModel>, timings: &mut Timings) {
    let (elf, lines) = parse_elf(bytes, timings);
    check_large_shape(&elf, &lines, map.is_some());
    let mdf = parse_mdf(MDF);
    assert!(mdf.problems.is_empty());
    let table = mdf.regions();
    for sibling in [None, map]
        .into_iter()
        .take(if map.is_some() { 2 } else { 1 })
    {
        let name = if sibling.is_some() { "ELF+MAP" } else { "ELF" };
        let (regions, region_time) = timings.time(format!("{name}/regions"), || {
            space::regions_for_elf(&elf, Some(&table))
        });
        let (owners, owner_time) = timings.time(format!("{name}/owners"), || {
            space::owners(&elf, Some(&lines), sibling)
        });
        if sibling.is_some() {
            assert!(owners.provenance.from_map >= 3000);
        } else if map.is_some() {
            assert!(owners.provenance.from_sections >= 3000);
        } else {
            assert!(owners.provenance.from_symbols >= 4900);
            assert_eq!(owners.provenance.load_images, 1);
            assert!(owners.items.iter().any(|owner| owner.image));
            assert_eq!(owners.provenance.estimated, 0);
        }
        measure_model(
            timings,
            name,
            Model {
                owners,
                regions,
                origin: Origin::Elf {
                    elf: &elf,
                    map: sibling,
                },
                elapsed: region_time + owner_time,
            },
        );
    }
    measure_selection(&elf, map.is_some(), timings);
}

fn measure_selection(elf: &Elf, split: bool, timings: &mut Timings) {
    let cancel = AtomicBool::new(false);
    let query = Query {
        hide_noise: true,
        ..Query::default()
    };
    let (selection, _) = timings.time("select/hide_noise", || {
        select(elf, &query, &cancel).unwrap()
    });
    timings.limit_last(50);
    assert_eq!(selection.rows.len(), if split { 2700 } else { 5000 });
    assert_eq!(
        selection.rows.len() as u64 + selection.hidden,
        elf.symbols.len() as u64
    );
}

fn check_large_shape(elf: &Elf, lines: &Lines, split: bool) {
    assert!(elf.problems.is_empty(), "{:?}", elf.problems);
    assert!(lines.problems.is_empty(), "{:?}", lines.problems);
    assert!(lines.found() >= 2500);
    assert_eq!(elf.symbols.len(), if split { 56_000 } else { 99_000 });
    if split {
        assert!((3300..3340).contains(&elf.sections.len()));
        assert_eq!(
            elf.sections
                .iter()
                .filter(|section| section.name.starts_with("$fill"))
                .count(),
            9
        );
        assert!(
            elf.sections
                .iter()
                .any(|section| section.name.starts_with("[.data."))
        );
    } else {
        assert!((105..125).contains(&elf.sections.len()));
        assert_eq!(elf.segments.len(), 50);
        assert_eq!(
            elf.symbols
                .iter()
                .filter(|symbol| symbol.bind == rusty_automotive::Bind::Local
                    && symbol.size == 0
                    && elf.name(symbol).starts_with(".L"))
                .count(),
            88_000
        );
        assert_eq!(
            elf.symbols.iter().filter(|symbol| symbol.size > 0).count(),
            5000
        );
    }
}

#[test]
#[ignore = "release speed and memory budgets; run explicitly with --run-ignored all"]
fn tasking_split_layout_meets_rich_budgets() {
    let fixture = split_layout(56_000);
    let text = fixture.map();
    let bytes = fixture.build();
    let mut timings = Timings::default();
    let (map, _) = timings.time("MAP parse", || {
        index_map(text.as_bytes(), &AtomicBool::new(false), &mut |_| {}).unwrap()
    });
    timings.limit_last(300);
    assert!(map.problems.is_empty(), "{:?}", map.problems);
    assert_eq!(map.sections.len(), 3309);
    assert_eq!(map.contributions.len(), 3300);
    measure_elf(&bytes, Some(&map), &mut timings);
    let table = parse_mdf(MDF).regions();
    let (regions, region_time) =
        timings.time("MAP/regions", || space::regions_for_map(&map, Some(&table)));
    let (owners, owner_time) = timings.time("MAP/owners", || space::owners_of_map(&map));
    measure_model(
        &mut timings,
        "MAP",
        Model {
            owners,
            regions,
            origin: Origin::Map(&map),
            elapsed: region_time + owner_time,
        },
    );
    timings.finish();
}

#[test]
#[ignore = "release speed and memory budgets; run explicitly with --run-ignored all"]
fn gcc_merged_layout_meets_rich_budgets() {
    let bytes = merged_layout(99_000).build();
    let mut timings = Timings::default();
    measure_elf(&bytes, None, &mut timings);
    timings.finish();
}

#[test]
#[ignore = "release speed and memory budgets; run explicitly with --run-ignored all"]
fn gcc_merged_layout_with_210k_symbols_times_the_estimate_path() {
    let bytes = estimated_layout(210_000, 4000).build();
    let mut timings = Timings::default();
    let (elf, lines) = parse_elf(&bytes, &mut timings);
    assert!(elf.problems.is_empty(), "{:?}", elf.problems);
    assert_eq!(elf.symbols.len(), 210_000);
    let table = parse_mdf(MDF).regions();
    let (regions, region_time) =
        timings.time("ELF/regions", || space::regions_for_elf(&elf, Some(&table)));
    let (owners, owner_time) =
        timings.time("ELF/owners", || space::owners(&elf, Some(&lines), None));
    assert!(owners.provenance.estimated >= 2000);
    measure_model(
        &mut timings,
        "ELF",
        Model {
            owners,
            regions,
            origin: Origin::Elf {
                elf: &elf,
                map: None,
            },
            elapsed: region_time + owner_time,
        },
    );
    timings.finish();
}

#[test]
fn rich_small_fixtures_cover_classes_endianness_stripping_dwarf_and_siblings() {
    for (is_64, is_big) in [(false, false), (false, true), (true, false), (true, true)] {
        for (stripped, dwarf) in [(false, false), (false, true), (true, false), (true, true)] {
            let fixture = rich::small(is_64, is_big, stripped, dwarf);
            let map = index_map(
                fixture.map().as_bytes(),
                &AtomicBool::new(false),
                &mut |_| {},
            )
            .unwrap();
            let (elf, lines) = elf_build::read(&fixture.build()).unwrap();
            assert_eq!(
                elf.header.class,
                if is_64 { Class::Elf64 } else { Class::Elf32 }
            );
            assert_eq!(
                elf.header.endian,
                if is_big { Endian::Big } else { Endian::Little }
            );
            assert_eq!(elf.symbols.is_empty(), stripped);
            assert_eq!(lines.at(rich::ROM).is_some(), dwarf);
            assert!(map.problems.is_empty());
            check_small_model(&elf, &lines, None);
            check_small_model(&elf, &lines, Some(&map));
        }
    }
}

fn check_small_model(elf: &Elf, lines: &Lines, map: Option<&MapModel>) {
    let owners = space::owners(elf, Some(lines), map);
    let mdf = parse_mdf(MDF).regions();
    let mut tables = vec![None, Some(mdf.clone())];
    if let Some(map) = map {
        tables.push(Some(rusty_automotive::RegionTable::merge(map, &mdf)));
    }
    for table in tables {
        let regions = space::regions_for_elf(elf, table.as_ref());
        for region in regions.items {
            assert_eq!(
                owners
                    .in_span(region.span().unwrap())
                    .iter()
                    .map(|owner| owner.size)
                    .sum::<u64>(),
                region.used
            );
        }
    }
}
