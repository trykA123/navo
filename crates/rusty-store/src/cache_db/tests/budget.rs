use super::*;

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
}

#[test]
fn used_bytes_after_rejected_value_drops_the_old_entry() {
    let mut cache = mem_with_budget(T, 30);
    fill_abc(&mut cache);
    cache.put(T, b"a", &[5; 31]).unwrap();
    assert_eq!(cache.used_bytes(T).unwrap(), 20);
}

#[test]
fn used_bytes_after_clear_is_zero() {
    let mut cache = mem();
    fill_abc(&mut cache);
    cache.clear(T).unwrap();
    assert_eq!(cache.used_bytes(T).unwrap(), 0);
}

#[test]
fn entries_counts_the_rows_of_one_table() {
    let mut cache = mem_with_budget(T, 30);
    fill_abc(&mut cache);
    cache.put("other", b"x", b"y").unwrap();
    assert_eq!(cache.entries(T).unwrap(), 3);
}
