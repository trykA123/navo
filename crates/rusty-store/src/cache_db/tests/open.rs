use super::*;

const PAGE: usize = 4096;

fn garbage() -> Vec<u8> {
    (0..8192_u32).map(|i| (i * 31 + 7) as u8).collect()
}

fn open(dir: &TempDir) -> (CacheDb, CacheOpen) {
    CacheDb::open(&cache_path(dir)).unwrap()
}

fn open_report(dir: &TempDir) -> CacheOpen {
    open(dir).1
}

fn write_sqlite_with_damaged_page(path: &Path) {
    Connection::open(path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE filler (id INTEGER PRIMARY KEY, text TEXT);
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 2000)
             INSERT INTO filler (text) SELECT printf('row-%05d-with-some-padding-text', i) FROM n;",
        )
        .unwrap();
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
fn missing_file_is_created() {
    let dir = tmp();
    assert_eq!(open_report(&dir), CacheOpen::Created);
}

#[test]
fn missing_file_exists_after_open() {
    let dir = tmp();
    let _cache = open(&dir);
    assert!(cache_path(&dir).is_file());
}

#[test]
fn existing_file_is_opened() {
    let dir = tmp();
    drop(open(&dir));
    assert_eq!(open_report(&dir), CacheOpen::Opened);
}

#[test]
fn garbage_file_is_recreated() {
    let dir = tmp();
    fs::write(cache_path(&dir), garbage()).unwrap();
    let want = CacheOpen::Recreated {
        moved_to: corrupt_path(&dir, 1),
    };
    assert_eq!(open_report(&dir), want);
}

#[test]
fn garbage_file_is_moved_with_its_bytes() {
    let dir = tmp();
    fs::write(cache_path(&dir), garbage()).unwrap();
    let _cache = open(&dir);
    assert_eq!(fs::read(corrupt_path(&dir, 1)).unwrap(), garbage());
}

#[test]
fn recreated_file_opens_cleanly_next_time() {
    let dir = tmp();
    fs::write(cache_path(&dir), garbage()).unwrap();
    drop(open(&dir));
    assert_eq!(open_report(&dir), CacheOpen::Opened);
}

#[test]
fn recreated_cache_accepts_puts() {
    let dir = tmp();
    fs::write(cache_path(&dir), garbage()).unwrap();
    let (mut cache, _) = open(&dir);
    cache.put("thumbs", b"a", b"one").unwrap();
    assert_eq!(cache.get("thumbs", b"a").unwrap(), Some(b"one".to_vec()));
}

#[test]
fn second_corruption_moves_to_corrupt_2() {
    let dir = tmp();
    fs::write(cache_path(&dir), garbage()).unwrap();
    drop(open(&dir));
    fs::write(cache_path(&dir), garbage()).unwrap();
    let want = CacheOpen::Recreated {
        moved_to: corrupt_path(&dir, 2),
    };
    assert_eq!(open_report(&dir), want);
}

#[test]
fn second_corruption_keeps_the_first_moved_file() {
    let dir = tmp();
    fs::write(cache_path(&dir), garbage()).unwrap();
    drop(open(&dir));
    fs::write(cache_path(&dir), b"second").unwrap();
    drop(open(&dir));
    assert_eq!(fs::read(corrupt_path(&dir, 1)).unwrap(), garbage());
}

#[test]
fn file_failing_quick_check_is_recreated() {
    let dir = tmp();
    write_sqlite_with_damaged_page(&cache_path(&dir));
    let want = CacheOpen::Recreated {
        moved_to: corrupt_path(&dir, 1),
    };
    assert_eq!(open_report(&dir), want);
}

#[test]
fn data_survives_close_and_reopen() {
    let dir = tmp();
    let (mut cache, _) = open(&dir);
    cache.put("thumbs", b"a", b"one").unwrap();
    drop(cache);
    let (cache, _) = open(&dir);
    assert_eq!(cache.get("thumbs", b"a").unwrap(), Some(b"one".to_vec()));
}

#[test]
fn used_bytes_survive_close_and_reopen() {
    let dir = tmp();
    let (mut cache, _) = open(&dir);
    cache.put("thumbs", b"abc", b"12345").unwrap();
    drop(cache);
    let (cache, _) = open(&dir);
    assert_eq!(cache.used_bytes("thumbs").unwrap(), 8);
}
