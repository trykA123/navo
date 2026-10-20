use std::fs;
use std::path::Path;

use rusty_store::Db;

const PAGE: usize = 4096;

pub(crate) fn fill_history(db: &Path) {
    let (db, _) = Db::open(db, false).unwrap();
    db.connection()
        .execute_batch(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 2000)
             INSERT INTO history (location, visited_at)
             SELECT printf('file:///home/admin/folder-%05d/with/a/longer/path', i), i FROM n;",
        )
        .unwrap();
}

pub(crate) fn root_page(db: &Path, table: &str) -> usize {
    let (db, _) = Db::open(db, false).unwrap();
    let page: i64 = db
        .connection()
        .query_row(
            "SELECT rootpage FROM sqlite_master WHERE name = ?1",
            [table],
            |row| row.get(0),
        )
        .unwrap();
    usize::try_from(page).unwrap()
}

pub(crate) fn corrupt_middle_page(db: &Path) {
    let pages = fs::read(db).unwrap().len() / PAGE;
    assert!(pages >= 8, "only {pages} pages");
    corrupt_page(db, pages / 2 + 1);
}

pub(crate) fn corrupt_page(db: &Path, page: usize) {
    let mut bytes = fs::read(db).unwrap();
    let start = (page - 1) * PAGE;
    for (i, byte) in bytes[start..start + PAGE].iter_mut().enumerate() {
        *byte = (i * 31 + 7) as u8;
    }
    fs::write(db, bytes).unwrap();
}
