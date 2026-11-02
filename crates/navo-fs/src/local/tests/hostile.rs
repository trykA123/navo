use std::collections::{HashMap, HashSet};

use crate::hostile_names::{self, Family, HostileName};

use super::*;

#[cfg(unix)]
const FAMILY: Family = Family::Unix;
#[cfg(windows)]
const FAMILY: Family = Family::Windows;
const WINDOWS_MAX_PATH: usize = 260;

fn kind_for(index: usize) -> EntryKind {
    if index.is_multiple_of(2) {
        EntryKind::File
    } else {
        EntryKind::Folder
    }
}

fn create(location: &Location, kind: EntryKind) {
    let path = os_path(location).unwrap();
    match kind {
        EntryKind::Folder => fs::create_dir(&path).unwrap(),
        _ => fs::write(&path, b"x").unwrap(),
    }
}

fn corpus_folder() -> (TempDir, Location, Vec<HostileName>) {
    let (dir, folder) = temp();
    let corpus = hostile_names::for_family(FAMILY);
    for (index, hostile) in corpus.iter().enumerate() {
        create(&folder.join(&hostile.name).unwrap(), kind_for(index));
    }
    (dir, folder, corpus)
}

fn expected(corpus: &[HostileName]) -> HashMap<ProviderPath, EntryKind> {
    corpus
        .iter()
        .enumerate()
        .map(|(index, hostile)| (hostile.name.clone(), kind_for(index)))
        .collect()
}

#[test]
fn listing_returns_every_hostile_name_exactly() {
    let (_dir, folder, corpus) = corpus_folder();
    let listed: HashSet<ProviderPath> = entries(&list_all(&folder).1)
        .into_iter()
        .map(|e| e.name)
        .collect();
    let names: HashSet<ProviderPath> = corpus.into_iter().map(|h| h.name).collect();
    assert_eq!(listed, names);
}

#[test]
fn listing_returns_one_entry_per_hostile_name() {
    let (_dir, folder, corpus) = corpus_folder();
    assert_eq!(entries(&list_all(&folder).1).len(), corpus.len());
}

#[test]
fn listing_keeps_each_hostile_kind() {
    let (_dir, folder, corpus) = corpus_folder();
    let listed: HashMap<ProviderPath, EntryKind> = entries(&list_all(&folder).1)
        .into_iter()
        .map(|e| (e.name, e.kind))
        .collect();
    assert_eq!(listed, expected(&corpus));
}

#[test]
fn metadata_of_each_hostile_child_has_its_name_and_kind() {
    let (_dir, folder, corpus) = corpus_folder();
    let fs = LocalFs::new();
    let found: HashMap<ProviderPath, EntryKind> = corpus
        .iter()
        .map(|hostile| {
            let entry = fs.metadata(&folder.join(&hostile.name).unwrap()).unwrap();
            (entry.name, entry.kind)
        })
        .collect();
    assert_eq!(found, expected(&corpus));
}

fn long_tree() -> (TempDir, Vec<Location>) {
    let (dir, root) = temp();
    let mut levels = vec![root];
    for segment in hostile_names::long_path_segments() {
        let next = levels.last().unwrap().join(&segment).unwrap();
        create(&next, EntryKind::Folder);
        levels.push(next);
    }
    let file = levels
        .last()
        .unwrap()
        .join(&ProviderPath::from_text("leaf.txt"))
        .unwrap();
    create(&file, EntryKind::File);
    let units = file.path().to_wide().map_or(0, |w| w.len());
    assert!(
        units > WINDOWS_MAX_PATH,
        "fixture path is only {units} units"
    );
    (dir, levels)
}

#[test]
fn every_level_of_a_long_path_lists_its_child() {
    let (_dir, levels) = long_tree();
    let mut children: Vec<String> = hostile_names::long_path_segments()
        .iter()
        .map(ProviderPath::to_text)
        .collect();
    children.push("leaf.txt".to_owned());
    let listed: Vec<Vec<String>> = levels
        .iter()
        .map(|level| sorted_names(&list_all(level).1))
        .collect();
    let wanted: Vec<Vec<String>> = children.into_iter().map(|c| vec![c]).collect();
    assert_eq!(listed, wanted);
}
