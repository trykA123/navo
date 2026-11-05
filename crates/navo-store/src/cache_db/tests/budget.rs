use super::*;
use crate::cache_db::budget::USED_BYTES_SQL;

const T: &str = "thumbs";

fn ten(byte: u8) -> [u8; 9] {
    [byte; 9]
}

fn fill_abc(cache: &mut CacheDb) {
    for key in [b"a", b"b", b"c"] {
        cache.put(T, key, &ten(key[0])).unwrap();
    }
}

fn present(cache: &CacheDb, keys: &[&[u8]]) -> Vec<bool> {
    keys.iter().map(|key| has(cache, T, key)).collect()
}

fn assert_totals(cache: &CacheDb, table: &str) {
    let sum: i64 = cache
        .conn
        .query_row(
            "SELECT COALESCE(SUM(size), 0) FROM blob WHERE tbl = ?1",
            [table],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        cache.used_bytes(table).unwrap(),
        u64::try_from(sum).unwrap(),
        "stored total for {table}"
    );
}

#[test]
fn eviction_drops_the_least_recently_put_entry() {
    let mut cache = mem_with_budget(T, 30);
    fill_abc(&mut cache);
    cache.put(T, b"d", &ten(4)).unwrap();
    assert_eq!(
        present(&cache, &[b"a", b"b", b"c", b"d"]),
        [false, true, true, true]
    );
}

#[test]
fn get_refreshes_an_entry() {
    let mut cache = mem_with_budget(T, 30);
    fill_abc(&mut cache);
    cache.get(T, b"a").unwrap();
    cache.put(T, b"d", &ten(4)).unwrap();
    assert_eq!(
        present(&cache, &[b"a", b"b", b"c", b"d"]),
        [true, false, true, true]
    );
}

#[test]
fn put_of_existing_key_refreshes_it() {
    let mut cache = mem_with_budget(T, 30);
    fill_abc(&mut cache);
    cache.put(T, b"a", &ten(9)).unwrap();
    cache.put(T, b"d", &ten(4)).unwrap();
    assert_eq!(
        present(&cache, &[b"a", b"b", b"c", b"d"]),
        [true, false, true, true]
    );
}

#[test]
fn eviction_drops_as_many_entries_as_needed() {
    let mut cache = mem_with_budget(T, 30);
    fill_abc(&mut cache);
    cache.put(T, b"e", &[5; 19]).unwrap();
    assert_eq!(
        present(&cache, &[b"a", b"b", b"c", b"e"]),
        [false, false, true, true]
    );
}

#[test]
fn entry_of_exactly_the_budget_is_stored() {
    let mut cache = mem_with_budget(T, 30);
    cache.put(T, b"a", &[1; 29]).unwrap();
    assert!(has(&cache, T, b"a"));
}

#[test]
fn value_above_the_budget_is_not_stored() {
    let mut cache = mem_with_budget(T, 30);
    cache.put(T, b"a", &[1; 31]).unwrap();
    assert!(!has(&cache, T, b"a"));
}

#[test]
fn key_and_value_above_the_budget_are_not_stored() {
    let mut cache = mem_with_budget(T, 30);
    cache.put(T, b"ab", &[1; 29]).unwrap();
    assert!(!has(&cache, T, b"ab"));
}

#[test]
fn value_above_the_budget_removes_the_old_value() {
    let mut cache = mem_with_budget(T, 30);
    cache.put(T, b"a", &ten(1)).unwrap();
    cache.put(T, b"a", &[2; 31]).unwrap();
    assert_eq!(cache.get(T, b"a").unwrap(), None);
}

#[test]
fn value_above_the_budget_keeps_other_entries() {
    let mut cache = mem_with_budget(T, 30);
    fill_abc(&mut cache);
    cache.put(T, b"e", &[5; 31]).unwrap();
    assert_eq!(present(&cache, &[b"a", b"b", b"c"]), [true, true, true]);
}

#[test]
fn tables_have_independent_budgets() {
    let mut cache = mem_with_budget(T, 20);
    cache.set_budget("sizes", 100);
    for key in [b"a", b"b", b"c"] {
        cache.put(T, key, &ten(1)).unwrap();
        cache.put("sizes", key, &ten(1)).unwrap();
    }
    assert_eq!(
        (
            cache.used_bytes(T).unwrap(),
            cache.used_bytes("sizes").unwrap()
        ),
        (20, 30)
    );
}

#[test]
fn a_full_table_does_not_evict_from_another() {
    let mut cache = mem_with_budget(T, 30);
    cache.set_budget("sizes", 30);
    cache.put("sizes", b"a", &ten(1)).unwrap();
    fill_abc(&mut cache);
    cache.put(T, b"d", &ten(4)).unwrap();
    assert!(has(&cache, "sizes", b"a"));
}

#[test]
fn used_bytes_of_empty_table_is_zero() {
    let cache = mem();
    assert_eq!(cache.used_bytes(T).unwrap(), 0);
}

#[test]
fn stored_totals_match_the_blob_sum_after_each_write() {
    let mut cache = mem();
    assert_totals(&cache, T);
    cache.put(T, b"a", b"one").unwrap();
    assert_totals(&cache, T);
    cache.put(T, b"a", b"second").unwrap();
    assert_totals(&cache, T);
    cache.put(T, b"b", b"two").unwrap();
    assert_totals(&cache, T);
    cache.remove(T, b"a").unwrap();
    assert_totals(&cache, T);
    cache.clear(T).unwrap();
    assert_totals(&cache, T);
}

#[test]
fn stored_totals_are_shared_across_two_connections() {
    let dir = tmp();
    let path = cache_path(&dir);
    let (mut writer, _) = CacheDb::open(&path).unwrap();
    let (mut reader, _) = CacheDb::open(&path).unwrap();
    writer.put(T, b"a", b"one").unwrap();
    assert_totals(&reader, T);
    reader.put(T, b"a", b"two").unwrap();
    assert_totals(&writer, T);
    writer.remove(T, b"a").unwrap();
    assert_totals(&reader, T);
}

#[test]
fn stored_totals_follow_direct_blob_writes() {
    let cache = mem();
    cache
        .conn
        .execute(
            "INSERT INTO blob (tbl, key, value, size, accessed) VALUES (?1, ?2, ?3, 9, 1)",
            params![T, b"a", b"one"],
        )
        .unwrap();
    assert_totals(&cache, T);
    cache
        .conn
        .execute("DELETE FROM blob WHERE tbl = ?1", [T])
        .unwrap();
    assert_totals(&cache, T);
}

#[test]
fn opening_an_unversioned_cache_fills_the_stored_totals() {
    let dir = tmp();
    let path = cache_path(&dir);
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE blob (
                 tbl TEXT, key BLOB, value BLOB, size INTEGER, accessed INTEGER,
                 PRIMARY KEY(tbl, key)
             );
             CREATE INDEX blob_age ON blob (tbl, accessed, key, size);
             INSERT INTO blob (tbl, key, value, size, accessed) VALUES
                 ('thumbs', x'61', x'6f6e65', 4, 1),
                 ('thumbs', x'62', x'74776f', 4, 2),
                 ('sizes', x'78', x'7979', 3, 3);",
        )
        .unwrap();
    let (mut cache, _) = CacheDb::open(&path).unwrap();
    assert_totals(&cache, "thumbs");
    assert_totals(&cache, "sizes");
    let version: u32 = cache
        .conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1);
    cache.put("thumbs", b"c", b"three").unwrap();
    assert_totals(&cache, "thumbs");
    cache.remove("thumbs", b"a").unwrap();
    assert_totals(&cache, "thumbs");
}

#[test]
fn five_thousand_puts_keep_the_stored_totals_exact() {
    let mut cache = mem();
    for index in 0..5_000_u32 {
        cache.put(T, &index.to_le_bytes(), b"value").unwrap();
    }
    assert_eq!(cache.used_bytes(T).unwrap(), 5_000 * 9);
    assert_totals(&cache, T);
}

#[test]
fn used_bytes_is_key_plus_value_length() {
    let mut cache = mem();
    cache.put(T, b"abc", b"12345").unwrap();
    cache.put(T, b"d", b"").unwrap();
    assert_eq!(cache.used_bytes(T).unwrap(), 9);
}

#[test]
fn used_bytes_drops_after_remove() {
    let mut cache = mem();
    cache.put(T, b"abc", b"12345").unwrap();
    cache.put(T, b"d", b"6").unwrap();
    cache.remove(T, b"abc").unwrap();
    assert_eq!(cache.used_bytes(T).unwrap(), 2);
}

#[test]
fn used_bytes_after_eviction_counts_survivors_only() {
    let mut cache = mem_with_budget(T, 30);
    fill_abc(&mut cache);
    cache.put(T, b"e", &[5; 19]).unwrap();
    assert_eq!(cache.used_bytes(T).unwrap(), 30);
    assert_totals(&cache, T);
}

#[test]
fn used_bytes_after_rejected_value_drops_the_old_entry() {
    let mut cache = mem_with_budget(T, 30);
    fill_abc(&mut cache);
    cache.put(T, b"a", &[5; 31]).unwrap();
    assert_eq!(cache.used_bytes(T).unwrap(), 20);
    assert_totals(&cache, T);
}

#[test]
fn used_bytes_after_clear_is_zero() {
    let mut cache = mem();
    fill_abc(&mut cache);
    cache.clear(T).unwrap();
    assert_eq!(cache.used_bytes(T).unwrap(), 0);
    assert_totals(&cache, T);
}

#[test]
fn usage_gives_the_bytes_and_rows_of_one_table() {
    let mut cache = mem_with_budget(T, 30);
    fill_abc(&mut cache);
    cache.put("other", b"x", b"y").unwrap();
    assert_eq!(cache.usage(T).unwrap(), (30, 3));
}

#[test]
fn budget_queries_do_not_sum_the_blobs() {
    let cache = mem_with_budget(T, 30);
    let mut statement = cache
        .conn
        .prepare(&format!("EXPLAIN QUERY PLAN {USED_BYTES_SQL}"))
        .unwrap();
    let total = statement
        .query_map([T], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n");
    assert!(total.contains("tbl_bytes"), "{total}");
    let mut statement = cache
        .conn
        .prepare(
            "EXPLAIN QUERY PLAN
             SELECT key FROM blob WHERE tbl = 'thumbs' ORDER BY accessed, key LIMIT 1",
        )
        .unwrap();
    let oldest = statement
        .query_map([], |row| row.get::<_, String>(3))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
        .join("\n");
    assert!(oldest.contains("COVERING INDEX blob_age"), "{oldest}");
}
