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
