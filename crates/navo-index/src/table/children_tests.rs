use super::super::compact::Remap;
use super::super::tests::tree;
use super::super::{Flags, NO_PARENT, NameTable};
use super::{Children, NO_LINK};
use crate::heap_probe::Probe;

/// Remap and new parents from a compacted copy, before the original is corrupted.
fn compacted(t: &NameTable) -> (Remap, Vec<u32>) {
    let mut copy = t.clone();
    let map = copy.compact();
    (map, copy.parents)
}

fn remapped(t: &NameTable, map: &Remap, parents: &[u32]) -> Option<Children> {
    t.children.remapped(map, parents)
}

#[test]
fn a_million_files_under_one_folder_hold_at_most_eight_bytes_per_slot() {
    let mut t = NameTable::new();
    let root = t.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let folder = t.push(root, "files", Flags::FOLDER).unwrap();
    for number in 0..1_000_000 {
        t.push(folder, &format!("entry-{number:07}.txt"), Flags::NONE)
            .unwrap();
    }
    t.shrink_to_fit();
    let held = t.children.memory_bytes();
    assert!(
        held <= 8 * t.slots() as usize + 64 * 1_024,
        "{held} B for {} slots",
        t.slots()
    );
}

#[test]
fn of_a_file_with_no_children_is_empty_and_has_no_row() {
    let (t, [.., notes, _]) = tree();
    assert_eq!(t.children.of(notes).count(), 0);
    assert!(t.children.heads.iter().all(|&(id, _)| id != notes));
}

#[test]
fn a_child_attached_to_an_id_pushed_as_a_file_is_listed_and_gets_a_row() {
    let (mut t, [.., notes, _]) = tree();
    let child = t.push(notes, "inner.txt", Flags::NONE).unwrap();
    assert_eq!(t.children.of(notes).collect::<Vec<_>>(), [child]);
    assert!(t.children.heads.iter().any(|&(id, _)| id == notes));
    assert!(t.children.heads.is_sorted_by_key(|&(id, _)| id));
}

#[test]
fn rebuild_scratch_stays_within_one_bit_per_slot() {
    let mut t = NameTable::new();
    let root = t.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let mut folder = root;
    for number in 1..1_000_000u32 {
        let flags = if number % 10 == 0 {
            Flags::FOLDER
        } else {
            Flags::NONE
        };
        let parent = if flags.contains(Flags::FOLDER) {
            root
        } else {
            folder
        };
        let id = t
            .push(parent, &format!("entry-{number:07}"), flags)
            .unwrap();
        if flags.contains(Flags::FOLDER) {
            folder = id;
        }
    }
    let flags = &t.flags;
    let probe = Probe::start();
    let rebuilt = Children::rebuild(
        &t.parents,
        |id| flags[id as usize] & Flags::DELETED == 0,
        |id| flags[id as usize] & Flags::FOLDER.stored() != 0,
    );
    let scratch = probe.read().peak - rebuilt.memory_bytes();
    let slots = t.slots() as usize;
    assert!(
        scratch <= slots / 8 + 4_096,
        "{scratch} B for {slots} slots"
    );
    assert_eq!(rebuilt.heads.len(), slots / 10);
}

#[test]
fn an_out_of_range_next_link_returns_none() {
    let (mut t, [_, _, _, notes, _]) = tree();
    let (map, parents) = compacted(&t);
    t.children.next[notes as usize] = t.slots() + 5;
    assert!(remapped(&t, &map, &parents).is_none());
}

#[test]
fn a_self_looping_list_returns_none() {
    let (mut t, [_, _, _, notes, _]) = tree();
    let (map, parents) = compacted(&t);
    t.children.next[notes as usize] = notes;
    assert!(remapped(&t, &map, &parents).is_none());
}

#[test]
fn a_two_entry_cycle_returns_none() {
    let (mut t, [_, _, ann, notes, docs]) = tree();
    let (map, parents) = compacted(&t);
    assert_eq!(t.children.of(ann).collect::<Vec<_>>(), [docs, notes]);
    t.children.next[notes as usize] = docs;
    assert!(remapped(&t, &map, &parents).is_none());
}

#[test]
fn a_removed_child_in_a_live_list_is_skipped_and_the_others_keep_their_order() {
    let (mut t, [_, _, ann, notes, docs]) = tree();
    let extra = t.push(ann, "extra.txt", Flags::NONE).unwrap();
    t.rename(notes, ann, "notes.md").unwrap();
    assert_eq!(t.children.of(ann).collect::<Vec<_>>(), [notes, extra, docs]);
    t.mark_deleted(extra);
    let (map, parents) = compacted(&t);
    let fresh = remapped(&t, &map, &parents).unwrap();
    let (notes, docs) = (map.get(notes).unwrap(), map.get(docs).unwrap());
    assert_eq!(
        fresh.of(map.get(ann).unwrap()).collect::<Vec<_>>(),
        [notes, docs]
    );
}

#[test]
fn a_waiting_child_stays_unlinked_and_the_count_still_matches() {
    let (mut t, [root, users, ..]) = tree();
    let waiting = t.push(t.slots() + 10, "orphan", Flags::NONE).unwrap();
    assert_eq!(t.children.waiting, [waiting]);
    let (map, parents) = compacted(&t);
    let fresh = remapped(&t, &map, &parents).unwrap();
    let orphan = map.get(waiting).unwrap();
    assert_eq!(parents[orphan as usize], super::NO_PARENT);
    assert!((0..parents.len() as u32).all(|p| fresh.of(p).all(|c| c != orphan)));
    assert_eq!(fresh.previous[orphan as usize], NO_LINK);
    assert_eq!(
        fresh.of(map.get(root).unwrap()).collect::<Vec<_>>(),
        [map.get(users).unwrap()]
    );
}
