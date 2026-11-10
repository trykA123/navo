//! Tests for natural name order, directly and through the listing sort.

use std::cmp::Ordering;

use navo_core::ProviderPath;

use super::{NaturalArena, natural_cmp};
use crate::view::{SortBy, SortKey, sort_order};
use crate::{Attributes, CancelToken, EntryKind, FsEntry};

fn sorted(by: SortBy, names: &[&str]) -> Vec<String> {
    let rows: Vec<FsEntry> = names
        .iter()
        .map(|name| FsEntry {
            name: ProviderPath::from_text(name),
            kind: EntryKind::File,
            size: 0,
            modified_ms: None,
            attributes: Attributes::default(),
        })
        .collect();
    let key = SortKey {
        by,
        descending: false,
    };
    let order = sort_order(&rows, key, false, &CancelToken::new()).unwrap();
    order
        .iter()
        .map(|&at| rows[at as usize].name.to_text())
        .collect()
}

#[test]
fn numbers_sort_by_value() {
    assert_eq!(
        sorted(SortBy::Name, &["file10", "file2", "file20", "file1"]),
        ["file1", "file2", "file10", "file20"]
    );
}

#[test]
fn leading_zeros_tie_break_last() {
    assert_eq!(
        sorted(SortBy::Name, &["file002", "file02", "file2", "file10"]),
        ["file2", "file02", "file002", "file10"]
    );
    assert_eq!(natural_cmp("a01b", "a1c"), Ordering::Less);
    assert_eq!(natural_cmp("a0", "a00"), Ordering::Less);
}

#[test]
fn digits_at_start_middle_and_end() {
    assert_eq!(
        sorted(SortBy::Name, &["10a", "9a", "x10y", "x9y", "z10", "z9"]),
        ["9a", "10a", "x9y", "x10y", "z9", "z10"]
    );
}

#[test]
fn long_digit_runs_never_overflow() {
    let small = format!("v{}1", "0".repeat(10) + &"9".repeat(25));
    let big = format!("v{}", "1".to_owned() + &"0".repeat(40));
    assert_eq!(natural_cmp(&small, &big), Ordering::Less);
    assert_eq!(natural_cmp(&big, &small), Ordering::Greater);
    assert_eq!(natural_cmp(&big, &big), Ordering::Equal);
}

#[test]
fn names_equal_but_for_case_keep_row_order() {
    assert_eq!(
        sorted(SortBy::Name, &["File1", "file1", "FILE1"]),
        ["File1", "file1", "FILE1"]
    );
}

#[test]
fn several_runs_compare_left_to_right() {
    assert_eq!(
        sorted(SortBy::Name, &["a1b10", "a1b2", "a2b1"]),
        ["a1b2", "a1b10", "a2b1"]
    );
}

#[test]
fn non_ascii_digits_stay_text() {
    assert_eq!(natural_cmp("a\u{0662}", "a\u{0661}0"), Ordering::Greater);
}

#[test]
fn extension_sort_uses_natural_names_inside_one_extension() {
    assert_eq!(
        sorted(SortBy::Ext, &["b10.txt", "a.zip", "b2.txt", "b1.txt"]),
        ["b1.txt", "b2.txt", "b10.txt", "a.zip"]
    );
}

#[test]
fn equal_keys_keep_the_index_order() {
    assert_eq!(
        sorted(SortBy::Name, &["x1", "x01", "x1", "x01"]),
        ["x1", "x1", "x01", "x01"]
    );
}

fn key_order(a: &str, b: &str) -> Ordering {
    let mut arena = NaturalArena::default();
    let (a, b) = (arena.add(a), arena.add(b));
    arena.compare(&a, &b)
}

/// The hand-written cases plus two runs longer than 20 digits.
fn fixed_names() -> Vec<String> {
    let long_a = format!("v{}1", "0".repeat(10) + &"9".repeat(25));
    let long_b = format!("v1{}", "0".repeat(40));
    let mut names: Vec<String> = [
        "file1",
        "file2",
        "file10",
        "file20",
        "file002",
        "file02",
        "a1b2",
        "a1b10",
        "a01b",
        "a1c",
        "a0",
        "a00",
        "10a",
        "9a",
        "x10y",
        "x9y",
        "z10",
        "z9",
        "a\u{0662}",
        "a\u{0661}0",
        "a1x",
        "a12",
        "a ",
        "a1",
        "a",
        "",
        "0",
        "00",
        "/1",
        ":1",
        "b.txt1",
    ]
    .iter()
    .map(|name| (*name).to_owned())
    .collect();
    names.extend([long_a, long_b]);
    names
}

#[test]
fn the_precomputed_key_agrees_with_natural_cmp() {
    let mut names = fixed_names();
    let mut seed = 0x9E37_79B9_7F4A_7C15_u64;
    for _ in 0..5_000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        names.push(format!(
            "{}{}{}{}",
            ["a", "b_", "Z", ""][(seed % 4) as usize],
            (seed >> 8) & 0xff,
            ["", ".x", "-", "0"][((seed >> 20) & 3) as usize],
            (seed >> 30) % 120
        ));
    }
    for pair in names.windows(2) {
        assert_eq!(
            key_order(&pair[0], &pair[1]),
            natural_cmp(&pair[0], &pair[1]),
            "{pair:?}"
        );
    }
    for (a, b) in names.iter().zip(names.iter().rev()) {
        assert_eq!(key_order(a, b), natural_cmp(a, b), "{a:?} {b:?}");
    }
    for a in names.iter().take(60) {
        for b in names.iter().take(60) {
            assert_eq!(key_order(a, b), natural_cmp(a, b), "{a:?} {b:?}");
        }
    }
}
