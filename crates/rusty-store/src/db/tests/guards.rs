use super::*;

const PAGE: usize = 4096;

fn build_current_with_rows(path: &Path) {
    build_at_version(path, Db::VERSION);
    Connection::open(path)
        .unwrap()
        .execute_batch(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 2000)
             INSERT INTO history (location, visited_at)
             SELECT printf('file:///home/admin/folder-%05d/with/a/longer/path', i), i FROM n;",
        )
        .unwrap();
}

fn corrupt_middle_page(path: &Path) {
    let mut bytes = fs::read(path).unwrap();
    let pages = bytes.len() / PAGE;
    assert!(pages >= 8, "only {pages} pages");
    let start = pages / 2 * PAGE;
    for (i, byte) in bytes[start..start + PAGE].iter_mut().enumerate() {
        *byte = (i * 31 + 7) as u8;
    }
    fs::write(path, bytes).unwrap();
}

#[test]
fn newer_file_is_refused_as_too_new() {
    let dir = tmp();
    let path = db_path(&dir);
    set_user_version(&path, Db::VERSION + 1);
    let err = Db::open(&path, false).err().unwrap();
    assert!(
        matches!(err, DbError::TooNew { found, supported } if found == Db::VERSION + 1 && supported == Db::VERSION),
        "{err:?}"
    );
}

#[test]
fn newer_file_is_left_unchanged() {
    let dir = tmp();
    let path = db_path(&dir);
    set_user_version(&path, Db::VERSION + 1);
    let before = fs::read(&path).unwrap();
    let _ = Db::open(&path, false);
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn unclean_exit_checks_a_good_file() {
    let dir = tmp();
    let path = db_path(&dir);
    build_current_with_rows(&path);
    let (_, report) = Db::open(&path, true).unwrap();
    assert!(report.checked);
}

#[test]
fn clean_exit_skips_the_check() {
    let dir = tmp();
    let path = db_path(&dir);
    build_current_with_rows(&path);
    let (_, report) = Db::open(&path, false).unwrap();
    assert!(!report.checked);
}

// Garbage in a middle b-tree page makes integrity_check itself fail with SQLITE_CORRUPT.
#[test]
fn unclean_exit_refuses_a_corrupt_file() {
    let dir = tmp();
    let path = db_path(&dir);
    build_current_with_rows(&path);
    corrupt_middle_page(&path);
    let err = Db::open(&path, true).err().unwrap();
    assert!(
        matches!(&err, DbError::Sqlite(e) if e.sqlite_error_code() == Some(rusqlite::ErrorCode::DatabaseCorrupt)),
        "{err:?}"
    );
}

#[test]
fn unclean_exit_leaves_a_corrupt_file_unchanged() {
    let dir = tmp();
    let path = db_path(&dir);
    build_current_with_rows(&path);
    corrupt_middle_page(&path);
    let before = fs::read(&path).unwrap();
    let _ = Db::open(&path, true);
    assert_eq!(fs::read(&path).unwrap(), before);
}
