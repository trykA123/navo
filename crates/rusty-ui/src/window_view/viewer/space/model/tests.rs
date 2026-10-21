use std::sync::atomic::AtomicBool;

use rusty_automotive::elf_sample::{Spec, build, demo, firmware};
use rusty_automotive::space::{Grouping, Kind};
use rusty_automotive::{Elf, ElfReader, Watch, parse_mdf};

use super::build_model;
use crate::window_view::viewer::space::sibling::{SiblingNames, SiblingProblem, Siblings};

pub(in crate::window_view::viewer) const DEMO_MDF: &str = "\
space linear 1 8
chip pflash 1 8 0x10000 rom
chip dsram 2 8 0x8000 ram
map 1 0x8000 0x10000 1 0
map 1 0x20000000 0x8000 2 0
";

fn parse(bytes: Vec<u8>) -> Elf {
    let cancel = AtomicBool::new(false);
    let mut progress = |_| {};
    let mut watch = Watch {
        cancel: &cancel,
        progress: &mut progress,
    };
    ElfReader::new(std::io::Cursor::new(bytes))
        .parse(&mut watch)
        .expect("the sample parses")
}

fn with_mdf() -> Siblings {
    Siblings {
        mdf: Some(parse_mdf(DEMO_MDF)),
        names: SiblingNames {
            mdf: Some("app.mdf".into()),
            map: None,
            elf: None,
        },
        ..Siblings::default()
    }
}

#[test]
fn without_a_sibling_regions_are_clusters_with_used_only() {
    let elf = parse(demo().expect("demo"));
    let model = build_model(&elf, None, &Siblings::default(), None);
    assert_eq!(model.regions.len(), 2);
    assert!(model.regions.iter().all(|region| region.total.is_none()));
    assert_eq!(
        model.provenance,
        "regions from address clusters · owners from symbols · kinds from section flags"
    );
    assert_eq!(model.region_source, None);
}

#[test]
fn an_mdf_sibling_names_the_regions_and_the_provenance() {
    let elf = parse(demo().expect("demo"));
    let model = build_model(&elf, None, &with_mdf(), None);
    let names: Vec<&str> = model.regions.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["pflash", "dsram"]);
    assert_eq!(model.flash, Some(0));
    assert_eq!(model.regions[0].total, Some(0x10000));
    assert!(model.provenance.starts_with("regions from app.mdf · "));
    assert_eq!(model.outside, 0);
}

#[test]
fn the_treemap_adds_up_to_the_used_bytes_of_the_flash() {
    let elf = parse(demo().expect("demo"));
    let model = build_model(&elf, None, &with_mdf(), None);
    let flash = &model.regions[model.flash.expect("a flash region")];
    let (groups, items) = model.treemap();
    let total: u64 = items.iter().map(|item| item.value).sum();
    assert_eq!(total, flash.used);
    assert!(!groups.is_empty());
    assert!(flash.by_kind[Kind::Code.index()] > 0);
}

#[test]
fn an_unusable_grouping_falls_back_to_the_default() {
    let elf = parse(demo().expect("demo"));
    let model = build_model(&elf, None, &with_mdf(), Some(Grouping::ObjectFile));
    assert!(!model.usable[Grouping::ObjectFile as usize]);
    assert_eq!(model.grouping, Grouping::Module);
    let model = build_model(&elf, None, &with_mdf(), Some(Grouping::Section));
    assert_eq!(model.grouping, Grouping::Section);
}

#[test]
fn a_stripped_file_says_it_has_no_symbol_table() {
    let spec = Spec {
        symbols: None,
        ..firmware(false, false)
    };
    let elf = parse(build(&spec).expect("builds"));
    let model = build_model(&elf, None, &Siblings::default(), None);
    assert!(
        model.provenance.contains("no symbol table"),
        "{}",
        model.provenance
    );
}

#[test]
fn sibling_problems_join_the_provenance_line() {
    let elf = parse(demo().expect("demo"));
    let siblings = Siblings {
        problems: vec![SiblingProblem::NotRecognised {
            name: "app.map".into(),
        }],
        ..Siblings::default()
    };
    let model = build_model(&elf, None, &siblings, None);
    assert!(
        model
            .provenance
            .ends_with("app.map skipped: format not recognised"),
        "{}",
        model.provenance
    );
}
