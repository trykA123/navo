use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use rusty_automotive::elf_sample::demo;
use rusty_core::Location;

use super::{
    ELF_LIMIT, MAP_LIMIT, MDF_LIMIT, SiblingProblem, Siblings, Stem, load_siblings, stem_of,
};
use crate::window_view::viewer::map::tests::SAMPLE;
use crate::window_view::viewer::{FileHooks, Head};

const MDF: &str = "\
# invented memory definition
space linear 1 8
chip flash0 1 8 0x10000 rom
chip dsram0 2 8 0x8000 ram
map 1 0x80000000 0x10000 1 0
map 1 0x70000000 0x8000 2 0
";

const GNU_MAP: &str = "\
Archive member included to satisfy reference by file (symbol)

Memory Configuration

Name             Origin             Length             Attributes
FLASH            0x0000000008000000 0x0000000000010000 xr
";

#[derive(Clone)]
enum Entry {
    Text(&'static str),
    Sized(u64),
    Broken(&'static str),
    Binary(Vec<u8>),
}

#[derive(Default)]
struct Folder {
    files: HashMap<String, Entry>,
    reads: Arc<Mutex<Vec<(String, u64)>>>,
}

impl Folder {
    fn with(mut self, name: &str, entry: Entry) -> Self {
        self.files.insert(format!("/work/{name}"), entry);
        self
    }

    fn hooks(&self) -> FileHooks {
        let files = Arc::new(self.files.clone());
        let candidates = files.clone();
        let (heads, wholes, reads) = (files.clone(), files, self.reads.clone());
        FileHooks {
            companions: Arc::new(move |folder, _| {
                Ok(candidates
                    .keys()
                    .filter_map(|path| name_in(path, folder))
                    .collect())
            }),
            head: Arc::new(move |location| match heads.get(location.as_str()) {
                Some(Entry::Text(text) | Entry::Broken(text)) => Ok(Head {
                    bytes: text.as_bytes().iter().copied().take(8192).collect(),
                    len: text.len() as u64,
                }),
                Some(Entry::Binary(bytes)) => Ok(Head {
                    bytes: bytes.iter().copied().take(8192).collect(),
                    len: bytes.len() as u64,
                }),
                Some(Entry::Sized(len)) => Ok(Head {
                    bytes: Vec::new(),
                    len: *len,
                }),
                None => Err("Cannot open it: not found".to_owned()),
            }),
            bytes: Arc::new(move |location, limit| {
                reads
                    .lock()
                    .expect("reads")
                    .push((location.as_str().to_owned(), limit));
                match wholes.get(location.as_str()) {
                    Some(Entry::Text(text)) if text.len() as u64 <= limit => {
                        Ok(text.as_bytes().to_vec())
                    }
                    Some(Entry::Binary(bytes)) if bytes.len() as u64 <= limit => Ok(bytes.clone()),
                    Some(Entry::Broken(_)) => Err("Cannot read it: device error".to_owned()),
                    Some(_) => Err("Too large to open here.".to_owned()),
                    None => Err("Cannot open it: not found".to_owned()),
                }
            }),
            ..FileHooks::none()
        }
    }

    fn reads(&self) -> Vec<(String, u64)> {
        self.reads.lock().expect("reads").clone()
    }
}

fn name_in(path: &str, folder: &Location) -> Option<rusty_core::ProviderPath> {
    let location = Location::new(path).ok()?;
    if location.parent().as_ref() != Some(folder) {
        return None;
    }
    location.name()
}

fn stem(name: &str) -> Stem {
    stem_of(&Location::new(format!("/work/{name}")).expect("location")).expect("stem")
}

fn load_with_elf(folder: &Folder, open: &str) -> Siblings {
    let stem = stem(open).with_elf();
    load_siblings(&folder.hooks(), Some(&stem), &AtomicBool::new(false))
}

fn load(folder: &Folder, open: &str) -> Siblings {
    load_siblings(&folder.hooks(), Some(&stem(open)), &AtomicBool::new(false))
}

#[test]
fn the_only_mdf_with_a_different_stem_is_used() {
    let folder = Folder::default().with("memory.mdf", Entry::Text(MDF));
    let siblings = load(&folder, "demo.elf");
    assert!(siblings.mdf.is_some());
    assert_eq!(siblings.names.mdf.as_deref(), Some("memory.mdf"));
    assert!(siblings.names.only_mdf);
}

#[test]
fn the_only_map_with_a_different_stem_is_used() {
    let folder = Folder::default().with("build.map", Entry::Text(SAMPLE));
    let siblings = load(&folder, "demo.elf");
    assert!(siblings.map.is_some());
    assert_eq!(siblings.names.map.as_deref(), Some("build.map"));
    assert!(siblings.names.only_map);
}

#[test]
fn several_companions_with_the_same_extension_are_ambiguous() {
    let folder = Folder::default()
        .with("one.mdf", Entry::Text(MDF))
        .with("two.MDF", Entry::Text(MDF));
    let siblings = load(&folder, "demo.elf");
    assert!(siblings.mdf.is_none());
    assert!(folder.reads().is_empty());
}

#[test]
fn the_same_stem_wins_over_other_companions() {
    let folder = Folder::default()
        .with("demo.mdf", Entry::Text(MDF))
        .with("other.mdf", Entry::Text(MDF));
    let siblings = load(&folder, "demo.elf");
    assert_eq!(siblings.names.mdf.as_deref(), Some("demo.mdf"));
    assert!(!siblings.names.only_mdf);
}

#[test]
fn an_oversized_fallback_is_refused_before_the_body_is_read() {
    let folder = Folder::default().with("memory.mdf", Entry::Sized(MDF_LIMIT + 1));
    let siblings = load(&folder, "demo.elf");
    assert!(siblings.mdf.is_none());
    assert!(matches!(
        siblings.problems.as_slice(),
        [SiblingProblem::TooLarge { .. }]
    ));
    assert!(folder.reads().is_empty());
}

#[test]
fn a_fallback_from_another_toolchain_is_refused_before_the_body_is_read() {
    let folder = Folder::default().with("build.map", Entry::Text(GNU_MAP));
    let siblings = load(&folder, "demo.elf");
    assert!(siblings.map.is_none());
    assert!(matches!(
        siblings.problems.as_slice(),
        [SiblingProblem::NotRecognised { .. }]
    ));
    assert!(folder.reads().is_empty());
}

#[test]
fn a_different_stem_elf_is_never_guessed() {
    let folder = Folder::default().with("other.elf", Entry::Binary(tiny_elf()));
    assert!(load_with_elf(&folder, "demo.map").elf.is_none());
    assert!(folder.reads().is_empty());
}

#[test]
fn cancellation_during_a_folder_scan_stops_before_any_body_is_read() {
    let folder = Folder::default().with("memory.mdf", Entry::Text(MDF));
    let cancel = Arc::new(AtomicBool::new(false));
    let during = cancel.clone();
    let hooks = FileHooks {
        companions: Arc::new(move |_, _| {
            during.store(true, std::sync::atomic::Ordering::Relaxed);
            Ok(vec![rusty_core::ProviderPath::from_text("memory.mdf")])
        }),
        ..folder.hooks()
    };
    let siblings = load_siblings(&hooks, Some(&stem("demo.elf")), &cancel);
    assert!(siblings.mdf.is_none());
    assert!(folder.reads().is_empty());
}

#[test]
fn both_fallbacks_share_one_folder_scan() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let folder = Folder::default()
        .with("memory.mdf", Entry::Text(MDF))
        .with("build.map", Entry::Text(SAMPLE));
    let mut hooks = folder.hooks();
    let list = hooks.companions.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    hooks.companions = Arc::new(move |folder, cancel| {
        counted.fetch_add(1, Ordering::Relaxed);
        list(folder, cancel)
    });
    let siblings = load_siblings(&hooks, Some(&stem("demo.elf")), &AtomicBool::new(false));
    assert!(siblings.mdf.is_some() && siblings.map.is_some());
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn a_cancelled_lookup_reads_no_companions() {
    let folder = Folder::default().with("demo.mdf", Entry::Text(MDF));
    let siblings = load_siblings(
        &folder.hooks(),
        Some(&stem("demo.elf")),
        &AtomicBool::new(true),
    );
    assert!(siblings.mdf.is_none());
    assert!(folder.reads().is_empty());
}

#[test]
fn an_mdf_alone_gives_regions_and_no_map() {
    let folder = Folder::default().with("demo.mdf", Entry::Text(MDF));
    let siblings = load(&folder, "demo.elf");
    let mdf = siblings.mdf.expect("mdf");
    assert_eq!(mdf.chips.len(), 2);
    assert!(siblings.map.is_none());
    assert_eq!(siblings.names.mdf.as_deref(), Some("demo.mdf"));
    assert!(siblings.problems.is_empty());
    assert_eq!(folder.reads(), [("/work/demo.mdf".to_owned(), MDF_LIMIT)]);
}

#[test]
fn an_upper_case_map_is_found_and_parsed() {
    let folder = Folder::default().with("demo.MAP", Entry::Text(SAMPLE));
    let siblings = load(&folder, "demo.elf");
    assert!(!siblings.map.expect("map").sections.is_empty());
    assert_eq!(siblings.names.map.as_deref(), Some("demo.MAP"));
    assert_eq!(folder.reads(), [("/work/demo.MAP".to_owned(), MAP_LIMIT)]);
}

#[test]
fn the_lower_case_name_wins_when_both_exist() {
    let folder = Folder::default()
        .with("demo.MDF", Entry::Text("chip other 9 8 0x10 rom\n"))
        .with("demo.mdf", Entry::Text(MDF));
    let siblings = load(&folder, "demo.elf");
    assert_eq!(siblings.names.mdf.as_deref(), Some("demo.mdf"));
}

#[test]
fn a_map_the_tasking_sniffer_rejects_counts_as_absent() {
    let folder = Folder::default().with("demo.map", Entry::Text(GNU_MAP));
    let siblings = load(&folder, "demo.elf");
    assert!(siblings.map.is_none());
    assert!(siblings.names.map.is_none());
    assert_eq!(
        siblings.problems,
        [SiblingProblem::NotRecognised {
            name: "demo.map".to_owned()
        }]
    );
    assert!(folder.reads().is_empty());
}

#[test]
fn a_measurement_mdf_is_not_taken_for_a_memory_definition() {
    let folder = Folder::default().with("demo.mdf", Entry::Text("MDF     4.10    \0\0"));
    let siblings = load(&folder, "demo.elf");
    assert!(siblings.mdf.is_none());
    assert!(folder.reads().is_empty());
}

#[test]
fn a_map_over_64_mib_is_refused_without_reading_it() {
    let folder = Folder::default().with("demo.map", Entry::Sized(65 * 1024 * 1024));
    let siblings = load(&folder, "demo.elf");
    assert!(siblings.map.is_none());
    assert_eq!(
        siblings.problems,
        [SiblingProblem::TooLarge {
            name: "demo.map".to_owned(),
            limit: MAP_LIMIT
        }]
    );
    assert!(folder.reads().is_empty());
}

#[test]
fn an_mdf_over_1_mib_is_refused() {
    let folder = Folder::default().with("demo.mdf", Entry::Sized(MDF_LIMIT + 1));
    let siblings = load(&folder, "demo.elf");
    assert!(siblings.mdf.is_none());
    assert!(matches!(
        siblings.problems.as_slice(),
        [SiblingProblem::TooLarge { .. }]
    ));
}

#[test]
fn an_unreadable_sibling_counts_as_absent() {
    let folder = Folder::default()
        .with("demo.mdf", Entry::Broken(MDF))
        .with("demo.map", Entry::Text(SAMPLE));
    let siblings = load(&folder, "demo.elf");
    assert!(siblings.mdf.is_none());
    assert!(siblings.map.is_some());
    assert_eq!(
        siblings.problems,
        [SiblingProblem::Unreadable {
            name: "demo.mdf".to_owned()
        }]
    );
}

#[test]
fn no_sibling_gives_nothing_and_no_problem() {
    let folder = Folder::default().with("other.txt", Entry::Text(SAMPLE));
    let siblings = load(&folder, "demo.elf");
    assert!(siblings.mdf.is_none() && siblings.map.is_none());
    assert_eq!(siblings.names, super::SiblingNames::default());
    assert!(siblings.problems.is_empty());
    assert!(folder.reads().is_empty());
}

#[test]
fn an_open_map_finds_its_mdf_but_never_loads_itself() {
    let folder = Folder::default()
        .with("demo.MAP", Entry::Text(SAMPLE))
        .with("demo.mdf", Entry::Text(MDF));
    let siblings = load(&folder, "demo.MAP");
    assert!(siblings.mdf.is_some());
    assert!(siblings.map.is_none());
    assert_eq!(folder.reads(), [("/work/demo.mdf".to_owned(), MDF_LIMIT)]);
}

#[test]
fn the_stem_keeps_inner_dots_and_drops_only_the_extension() {
    let folder = Folder::default().with("app.v2.mdf", Entry::Text(MDF));
    assert!(load(&folder, "app.v2.elf").mdf.is_some());
}

#[test]
fn a_cancelled_load_skips_the_map() {
    let folder = Folder::default()
        .with("demo.mdf", Entry::Text(MDF))
        .with("demo.map", Entry::Text(SAMPLE));
    let siblings = load_siblings(
        &folder.hooks(),
        Some(&stem("demo.elf")),
        &AtomicBool::new(true),
    );
    assert!(siblings.map.is_none());
    assert!(
        folder
            .reads()
            .iter()
            .all(|(name, _)| name.ends_with(".mdf"))
    );
}

#[test]
fn a_location_without_a_folder_has_no_stem() {
    assert!(stem_of(&Location::new("/").expect("root")).is_none());
}

fn tiny_elf() -> Vec<u8> {
    demo().expect("the demo builds")
}

#[test]
fn an_open_map_loads_the_elf_beside_it() {
    let folder = Folder::default().with("demo.ELF", Entry::Binary(tiny_elf()));
    let siblings = load_with_elf(&folder, "demo.map");
    assert!(!siblings.elf.expect("elf").sections.is_empty());
    assert_eq!(siblings.names.elf.as_deref(), Some("demo.ELF"));
    assert_eq!(folder.reads(), [("/work/demo.ELF".to_owned(), ELF_LIMIT)]);
}

#[test]
fn an_open_elf_never_loads_an_elf_sibling() {
    let folder = Folder::default().with("demo.elf", Entry::Binary(tiny_elf()));
    assert!(load(&folder, "demo.map").elf.is_none());
    assert!(load_with_elf(&folder, "demo.elf").elf.is_none());
    assert!(folder.reads().is_empty());
}

#[test]
fn an_elf_sibling_that_is_not_an_elf_or_too_large_is_skipped_with_a_problem() {
    let folder = Folder::default().with("demo.elf", Entry::Text("not an elf"));
    let siblings = load_with_elf(&folder, "demo.map");
    assert!(siblings.elf.is_none());
    assert_eq!(
        siblings.problems,
        [SiblingProblem::NotRecognised {
            name: "demo.elf".to_owned()
        }]
    );
    let big = Folder::default().with("demo.elf", Entry::Sized(ELF_LIMIT + 1));
    assert!(matches!(
        load_with_elf(&big, "demo.map").problems.as_slice(),
        [SiblingProblem::TooLarge { .. }]
    ));
}

#[test]
fn a_truncated_elf_sibling_counts_as_unreadable() {
    let mut bytes = tiny_elf();
    bytes.truncate(40);
    let folder = Folder::default().with("demo.elf", Entry::Binary(bytes));
    let siblings = load_with_elf(&folder, "demo.map");
    assert!(siblings.elf.is_none());
    assert_eq!(
        siblings.problems,
        [SiblingProblem::Unreadable {
            name: "demo.elf".to_owned()
        }]
    );
}
