use super::{CREATE, DELETE, FIRST_FILE, NEW_NAME, OLD_NAME, ROOT, Tree, apply, record};
use crate::ntfs::{COMPACT_MIN_SLOTS, NtfsIndex};
use crate::table::Totals;
use crate::usn::UsnRecord;

const PARENT: u128 = 2_000;
const OLDER: u128 = 2_001;
const NEWER: u128 = 2_002;

fn sized(file: u128, parent: u128, bytes: u64) -> UsnRecord {
    UsnRecord {
        size: Some(bytes),
        ..record(file, parent, CREATE, "inside.txt")
    }
}

fn answer(index: &NtfsIndex) -> (Option<Totals>, Vec<String>) {
    let table = &index.table;
    let found = table
        .find_folder("C:/P/case")
        .and_then(|id| table.totals(id));
    let parent = index.ids.get(&PARENT).copied().unwrap_or_default();
    let names = table.child_names(parent).into_iter().map(str::to_owned);
    (found, names.collect())
}

#[test]
fn compaction_keeps_child_order_so_a_case_folded_lookup_finds_the_same_folder() {
    let files = u128::from(COMPACT_MIN_SLOTS) + 10_000;
    let mut tree = Tree::new(3, 0, files);
    apply(
        &mut tree.index,
        vec![
            record(PARENT, ROOT, CREATE, "P"),
            record(OLDER, PARENT, CREATE, "Case"),
            sized(FIRST_FILE * 5 + 1, OLDER, 11),
            record(NEWER, PARENT, CREATE, "case"),
            sized(FIRST_FILE * 5 + 2, NEWER, 22),
        ],
    );
    let renamed = vec![
        record(OLDER, PARENT, OLD_NAME, "Case"),
        record(OLDER, PARENT, NEW_NAME, "Case"),
    ];
    apply(&mut tree.index, renamed);
    let before = answer(&tree.index);
    assert_eq!(before.0.map(|totals| totals.bytes), Some(11));
    assert_eq!(before.1, ["Case", "case"]);
    let slots = tree.index.table.slots();
    let gone = (FIRST_FILE..FIRST_FILE + files).map(|file| record(file, ROOT, DELETE, "gone"));
    apply(&mut tree.index, gone.collect());
    assert!(tree.index.table.slots() < slots, "the batch compacted");
    assert_eq!(answer(&tree.index), before);
}
