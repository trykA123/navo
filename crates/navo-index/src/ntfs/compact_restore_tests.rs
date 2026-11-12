//! Checks that restoring a sparse NTFS snapshot preserves its entries while compacting storage.

use super::{
    FIRST_FILE, NtfsIndex, POSITION, ROOT, all_paths, assert_dense_and_consistent, file_size,
    header, listing,
};
use crate::file;
use crate::ntfs::COMPACT_MIN_SLOTS;
use crate::table::{Flags, NO_PARENT, NameTable};
use crate::usn::FrnMap;

#[test]
fn restoring_a_sparse_snapshot_compacts_it_and_keeps_the_listing() {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let mut ids: FrnMap = [(ROOT, root)].into();
    for number in 0..u128::from(COMPACT_MIN_SLOTS) + 10_000 {
        let id = table
            .push(root, &format!("s{number}.txt"), Flags::NONE)
            .unwrap();
        table.set_size(id, file_size(number)).unwrap();
        ids.insert(FIRST_FILE + number, id);
        if number % 5 < 3 {
            table.remove(id);
            ids.remove(&(FIRST_FILE + number));
        }
    }
    let mut sparse = NtfsIndex::new('C', POSITION).unwrap();
    sparse.table = table;
    sparse.ids = ids;
    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join("c.index");
    file::save_with_ids(&path, &header(), &sparse.table, &sparse.ids).unwrap();
    let restored = NtfsIndex::restore(file::load_with_ids(&path).unwrap(), 42, 7).unwrap();
    assert_dense_and_consistent(&restored);
    assert!(restored.table.slots() < sparse.table.slots());
    assert_eq!(listing(&restored), listing(&sparse));
    assert_eq!(all_paths(&restored), all_paths(&sparse));
}
