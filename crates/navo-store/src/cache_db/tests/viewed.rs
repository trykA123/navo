use super::*;

fn key(byte: u8) -> [u8; 64] {
    [byte; 64]
}

#[test]
fn a_viewed_mark_round_trips() {
    let mut cache = mem();
    assert!(!cache.get_viewed(&key(1)).unwrap());
    cache.put_viewed(&key(1), 1_700_000_000_000).unwrap();
    assert!(cache.get_viewed(&key(1)).unwrap());
}

#[test]
fn an_unknown_key_is_not_viewed() {
    let cache = mem();
    assert!(!cache.get_viewed(&key(9)).unwrap());
}

#[test]
fn clearing_a_mark_makes_the_pair_unviewed_again() {
    let mut cache = mem();
    cache.put_viewed(&key(1), 10).unwrap();
    cache.put_viewed(&key(2), 20).unwrap();
    assert!(cache.clear_viewed(&key(1)).unwrap());
    assert!(!cache.get_viewed(&key(1)).unwrap());
    assert!(cache.get_viewed(&key(2)).unwrap());
    assert!(!cache.clear_viewed(&key(1)).unwrap());
}

#[test]
fn the_set_time_is_stored_with_the_mark() {
    let mut cache = mem();
    let set_ms = 1_700_000_000_123_i64;
    cache.put_viewed(&key(3), set_ms).unwrap();
    let stored = cache.get("compare-viewed", &key(3)).unwrap();
    assert_eq!(stored, Some(set_ms.to_le_bytes().to_vec()));
}

#[test]
fn putting_a_key_again_updates_the_time_without_a_second_row() {
    let mut cache = mem();
    cache.put_viewed(&key(4), 10).unwrap();
    cache.put_viewed(&key(4), 20).unwrap();
    assert_eq!(cache.usage("compare-viewed").unwrap().1, 1);
    let stored = cache.get("compare-viewed", &key(4)).unwrap();
    assert_eq!(stored, Some(20_i64.to_le_bytes().to_vec()));
}

#[test]
fn budget_eviction_drops_the_oldest_mark() {
    let mut cache = mem();
    let entry_bytes = 64 + 8;
    cache.set_budget("compare-viewed", entry_bytes * 2);
    cache.put_viewed(&key(1), 10).unwrap();
    cache.put_viewed(&key(2), 20).unwrap();
    cache.put_viewed(&key(3), 30).unwrap();
    assert!(!cache.get_viewed(&key(1)).unwrap());
    assert!(cache.get_viewed(&key(2)).unwrap());
    assert!(cache.get_viewed(&key(3)).unwrap());
    assert!(cache.used_bytes("compare-viewed").unwrap() <= entry_bytes * 2);
}

#[test]
fn a_viewed_mark_survives_a_reopen() {
    let dir = tmp();
    let path = cache_path(&dir);
    {
        let (mut cache, _) = CacheDb::open(&path).unwrap();
        cache.put_viewed(&key(5), 55).unwrap();
    }
    let (cache, _) = CacheDb::open(&path).unwrap();
    assert!(cache.get_viewed(&key(5)).unwrap());
}

#[test]
fn the_table_budget_is_four_mib() {
    assert_eq!(COMPARE_VIEWED_BUDGET, 4 * 1024 * 1024);
}
