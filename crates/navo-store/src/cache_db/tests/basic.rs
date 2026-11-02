use super::*;

#[test]
fn put_then_get_returns_the_value() {
    let mut cache = mem();
    cache.put("thumbs", b"a", b"one").unwrap();
    assert_eq!(cache.get("thumbs", b"a").unwrap(), Some(b"one".to_vec()));
}

#[test]
fn get_of_missing_key_is_none() {
    let cache = mem();
    assert_eq!(cache.get("thumbs", b"missing").unwrap(), None);
}

#[test]
fn put_of_empty_value_is_stored() {
    let mut cache = mem();
    cache.put("thumbs", b"a", b"").unwrap();
    assert_eq!(cache.get("thumbs", b"a").unwrap(), Some(Vec::new()));
}

#[test]
fn replace_changes_the_value() {
    let mut cache = mem();
    cache.put("thumbs", b"a", b"one").unwrap();
    cache.put("thumbs", b"a", b"second").unwrap();
    assert_eq!(cache.get("thumbs", b"a").unwrap(), Some(b"second".to_vec()));
}

#[test]
fn replace_changes_the_size() {
    let mut cache = mem();
    cache.put("thumbs", b"a", b"one").unwrap();
    cache.put("thumbs", b"a", b"second").unwrap();
    assert_eq!(cache.used_bytes("thumbs").unwrap(), 7);
}

#[test]
fn remove_of_present_key_returns_true() {
    let mut cache = mem();
    cache.put("thumbs", b"a", b"one").unwrap();
    assert!(cache.remove("thumbs", b"a").unwrap());
}

#[test]
fn remove_deletes_the_value() {
    let mut cache = mem();
    cache.put("thumbs", b"a", b"one").unwrap();
    cache.remove("thumbs", b"a").unwrap();
    assert_eq!(cache.get("thumbs", b"a").unwrap(), None);
}

#[test]
fn second_remove_returns_false() {
    let mut cache = mem();
    cache.put("thumbs", b"a", b"one").unwrap();
    cache.remove("thumbs", b"a").unwrap();
    assert!(!cache.remove("thumbs", b"a").unwrap());
}

#[test]
fn clear_removes_every_entry_of_the_table() {
    let mut cache = mem();
    cache.put("thumbs", b"a", b"one").unwrap();
    cache.put("thumbs", b"b", b"two").unwrap();
    cache.clear("thumbs").unwrap();
    assert_eq!(
        (has(&cache, "thumbs", b"a"), has(&cache, "thumbs", b"b")),
        (false, false)
    );
}

#[test]
fn clear_leaves_other_tables() {
    let mut cache = mem();
    cache.put("thumbs", b"a", b"one").unwrap();
    cache.put("sizes", b"a", b"two").unwrap();
    cache.clear("thumbs").unwrap();
    assert_eq!(cache.get("sizes", b"a").unwrap(), Some(b"two".to_vec()));
}

#[test]
fn same_key_in_two_tables_is_separate() {
    let mut cache = mem();
    cache.put("thumbs", b"a", b"one").unwrap();
    cache.put("sizes", b"a", b"two").unwrap();
    assert_eq!(cache.get("thumbs", b"a").unwrap(), Some(b"one".to_vec()));
}

#[test]
fn empty_table_name_is_allowed() {
    let mut cache = mem();
    cache.put("", b"a", b"one").unwrap();
    assert_eq!(cache.get("", b"a").unwrap(), Some(b"one".to_vec()));
}

#[test]
fn empty_table_name_is_separate() {
    let mut cache = mem();
    cache.put("", b"a", b"one").unwrap();
    assert_eq!(cache.get("thumbs", b"a").unwrap(), None);
}

#[test]
fn keys_are_compared_as_bytes() {
    let mut cache = mem();
    cache.put("thumbs", &[0, 255], b"one").unwrap();
    cache.put("thumbs", &[0, 255, 0], b"two").unwrap();
    assert_eq!(
        cache.get("thumbs", &[0, 255]).unwrap(),
        Some(b"one".to_vec())
    );
}

fn age(cache: &CacheDb, table: &str, key: &[u8]) -> i64 {
    cache
        .conn
        .query_row(
            "SELECT accessed FROM blob WHERE tbl = ?1 AND key = ?2",
            params![table, key],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn cache_hits_do_not_write_access_ages() {
    let mut cache = mem();
    cache.put("listing", b"a", b"value").unwrap();
    let writes = cache.conn.total_changes();
    let before = age(&cache, "listing", b"a");
    for _ in 0..10 {
        assert!(cache.get("listing", b"a").unwrap().is_some());
    }
    assert_eq!(cache.conn.total_changes(), writes);
    assert_eq!(age(&cache, "listing", b"a"), before);
}

#[test]
fn eviction_pass_persists_access_ages() {
    let mut cache = mem();
    cache.put("listing", b"a", b"value").unwrap();
    let before = age(&cache, "listing", b"a");
    cache.get("listing", b"a").unwrap();
    cache.set_budget("listing", 100);
    assert!(age(&cache, "listing", b"a") > before);
}

#[test]
fn insertion_evicts_using_pending_access_ages() {
    let mut cache = mem_with_budget("listing", 4);
    cache.put("listing", b"a", b"1").unwrap();
    cache.put("listing", b"b", b"2").unwrap();
    cache.get("listing", b"a").unwrap();
    cache.put("listing", b"c", b"3").unwrap();
    assert!(has(&cache, "listing", b"a"));
    assert!(!has(&cache, "listing", b"b"));
    assert!(has(&cache, "listing", b"c"));
}

#[test]
fn closing_the_cache_persists_access_ages() {
    let dir = tmp();
    let path = cache_path(&dir);
    let (mut cache, _) = CacheDb::open(&path).unwrap();
    cache.put("listing", b"a", b"value").unwrap();
    let before = age(&cache, "listing", b"a");
    cache.get("listing", b"a").unwrap();
    drop(cache);
    let (cache, _) = CacheDb::open(&path).unwrap();
    assert!(age(&cache, "listing", b"a") > before);
}

#[test]
fn replacing_a_hit_keeps_the_newer_access_age() {
    let mut cache = mem();
    cache.put("listing", b"a", b"value").unwrap();
    cache.get("listing", b"a").unwrap();
    let pending = cache.clock.get();
    cache.put("listing", b"a", b"new value").unwrap();
    cache.set_budget("listing", 100);
    assert!(age(&cache, "listing", b"a") > pending);
}

fn caches_at_capacity() -> (TempDir, CacheDb, CacheDb) {
    let dir = tmp();
    let (mut writer, _) = CacheDb::open(&cache_path(&dir)).unwrap();
    for index in 0..MAX_PENDING_ACCESSES {
        writer
            .put("listing", &index.to_le_bytes(), b"value")
            .unwrap();
    }
    let (reader, _) = CacheDb::open(&cache_path(&dir)).unwrap();
    (dir, writer, reader)
}

#[test]
fn pending_accesses_flush_at_capacity_with_two_handles() {
    let (_dir, writer, reader) = caches_at_capacity();
    reader
        .conn
        .busy_timeout(Duration::from_millis(1375))
        .unwrap();
    let before = age(&writer, "listing", &0_usize.to_le_bytes());
    for index in 0..MAX_PENDING_ACCESSES {
        assert!(
            reader
                .get("listing", &index.to_le_bytes())
                .unwrap()
                .is_some()
        );
        assert!(reader.accesses.borrow().len() < MAX_PENDING_ACCESSES);
    }
    assert!(reader.accesses.borrow().is_empty());
    assert!(age(&writer, "listing", &0_usize.to_le_bytes()) > before);
    let timeout: i64 = reader
        .conn
        .pragma_query_value(None, "busy_timeout", |row| row.get(0))
        .unwrap();
    assert_eq!(timeout, 1375);
}

#[test]
fn busy_access_flush_drops_oldest_half_and_restores_timeout() {
    let (_dir, writer, reader) = caches_at_capacity();
    reader
        .conn
        .busy_timeout(Duration::from_millis(1375))
        .unwrap();
    writer.conn.execute_batch("BEGIN IMMEDIATE").unwrap();
    for index in 0..MAX_PENDING_ACCESSES {
        assert!(
            reader
                .get("listing", &index.to_le_bytes())
                .unwrap()
                .is_some()
        );
        assert!(reader.accesses.borrow().len() < MAX_PENDING_ACCESSES);
    }
    let pending = reader.accesses.borrow();
    assert_eq!(pending.len(), MAX_PENDING_ACCESSES / 2);
    for index in 0..MAX_PENDING_ACCESSES {
        assert_eq!(
            pending.contains_key(&(String::from("listing"), index.to_le_bytes().to_vec())),
            index >= MAX_PENDING_ACCESSES / 2
        );
    }
    drop(pending);
    let timeout: i64 = reader
        .conn
        .pragma_query_value(None, "busy_timeout", |row| row.get(0))
        .unwrap();
    assert_eq!(timeout, 1375);
    writer.conn.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn closing_a_busy_cache_stays_within_the_close_deadline() {
    let dir = tmp();
    let path = cache_path(&dir);
    let (mut cache, _) = CacheDb::open(&path).unwrap();
    cache.put("listing", b"a", b"value").unwrap();
    let before = age(&cache, "listing", b"a");
    cache.get("listing", b"a").unwrap();
    cache.conn.busy_timeout(Duration::from_secs(5)).unwrap();
    let writer = Connection::open(&path).unwrap();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = std::time::Instant::now();
    drop(cache);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "best-effort access ages must not exceed the close deadline"
    );
    writer.execute_batch("ROLLBACK").unwrap();
    let (cache, _) = CacheDb::open(&path).unwrap();
    assert_eq!(age(&cache, "listing", b"a"), before);
}
