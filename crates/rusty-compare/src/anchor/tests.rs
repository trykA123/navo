use imara_diff::{Algorithm, Diff, InternedInput};
use proptest::prelude::*;

use super::*;

fn candidates(keys: &[u16]) -> Vec<Candidate<u16>> {
    keys.iter()
        .enumerate()
        .map(|(position, &key)| Candidate {
            key,
            position: u64::try_from(position).unwrap(),
        })
        .collect()
}

fn check_cover(pairs: &[SectionPair], left: &Range<u64>, right: &Range<u64>) {
    let mut left_at = left.start;
    let mut right_at = right.start;
    for pair in pairs {
        assert_eq!(pair.left.start, left_at);
        assert_eq!(pair.right.start, right_at);
        assert!(pair.left.start < pair.left.end || pair.right.start < pair.right.end);
        left_at = pair.left.end;
        right_at = pair.right.end;
    }
    assert_eq!(left_at, left.end);
    assert_eq!(right_at, right.end);
}

fn text(lines: &[u32]) -> String {
    lines.iter().map(|line| format!("{line}\n")).collect()
}

fn equal_lines(diff: &Diff, left_len: u64, right_len: u64) -> Vec<(u64, u64)> {
    let mut pairs = Vec::new();
    let mut left = 0_u32;
    let mut right = 0_u32;
    for hunk in diff.hunks() {
        while left < hunk.before.start && right < hunk.after.start {
            pairs.push((u64::from(left), u64::from(right)));
            left += 1;
            right += 1;
        }
        left = hunk.before.end;
        right = hunk.after.end;
    }
    while u64::from(left) < left_len && u64::from(right) < right_len {
        pairs.push((u64::from(left), u64::from(right)));
        left += 1;
        right += 1;
    }
    pairs
}

fn unique(values: Vec<u16>) -> Vec<u16> {
    let mut unique = Vec::new();
    for value in values {
        if !unique.contains(&value) {
            unique.push(value);
        }
    }
    unique
}

fn sequence(base: &[u16], mask: u64, first_added: u32, added: usize) -> Vec<u32> {
    let mut lines = Vec::new();
    let mut inserted = 0;
    for (index, &value) in base.iter().enumerate() {
        if mask & (1 << index) == 0 {
            continue;
        }
        lines.push(u32::from(value));
        if inserted < added {
            lines.push(first_added + u32::try_from(inserted).unwrap());
            inserted += 1;
        }
    }
    lines
}

fn positions<K: Copy>(lines: &[K]) -> Vec<Candidate<K>> {
    lines
        .iter()
        .enumerate()
        .map(|(position, &key)| Candidate {
            key,
            position: u64::try_from(position).unwrap(),
        })
        .collect()
}

fn shuffle<T>(items: &mut [T], mut state: u64) {
    for index in (1..items.len()).rev() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let pick = usize::try_from(state >> 33).unwrap_or(usize::MAX) % (index + 1);
        items.swap(index, pick);
    }
}

fn overlapping_sides() -> impl Strategy<Value = (Vec<u8>, Vec<u8>)> {
    (
        prop::collection::vec(0_u8..5, 0..10),
        prop::collection::vec(0_u8..5, 0..5),
        prop::collection::vec(0_u8..5, 0..5),
        any::<u64>(),
        any::<u64>(),
    )
        .prop_map(|(base, left_added, right_added, left_seed, right_seed)| {
            let mut left = base.clone();
            left.extend(left_added);
            shuffle(&mut left, left_seed);
            let mut right = base;
            right.extend(right_added);
            shuffle(&mut right, right_seed);
            (left, right)
        })
}

fn count_of(lines: &[u8], key: u8) -> usize {
    lines.iter().filter(|line| **line == key).count()
}

fn oracle_anchor_count(left: &[u8], right: &[u8]) -> usize {
    let mut matches = Vec::new();
    for (left_at, &key) in left.iter().enumerate() {
        if count_of(left, key) != 1 {
            continue;
        }
        let mut right_at = None;
        for (index, &other) in right.iter().enumerate() {
            if other == key {
                right_at = Some(index);
            }
        }
        if count_of(right, key) == 1
            && let Some(right_at) = right_at
        {
            matches.push((left_at, right_at));
        }
    }
    let mut best = vec![1_usize; matches.len()];
    for index in 0..matches.len() {
        for before in 0..index {
            if matches[before].1 < matches[index].1 {
                best[index] = best[index].max(best[before] + 1);
            }
        }
    }
    best.iter().copied().max().unwrap_or(0)
}

fn histogram_equal(left: &[u32], right: &[u32]) -> Vec<(u64, u64)> {
    let left_text = text(left);
    let right_text = text(right);
    let input = InternedInput::new(left_text.as_str(), right_text.as_str());
    let diff = Diff::compute(Algorithm::Histogram, &input);
    equal_lines(
        &diff,
        u64::try_from(left.len()).unwrap(),
        u64::try_from(right.len()).unwrap(),
    )
}

#[test]
fn repeated_keys_never_anchor() {
    let left = candidates(&[5, 6, 5, 7]);
    let right = candidates(&[5, 6, 5, 7]);
    assert_eq!(
        anchors(&left, &right),
        vec![Anchor { left: 1, right: 1 }, Anchor { left: 3, right: 3 },]
    );
}

#[test]
fn keys_present_on_one_side_never_anchor() {
    let left = candidates(&[1, 2, 3]);
    let right = candidates(&[3, 4, 5]);
    assert_eq!(anchors(&left, &right), vec![Anchor { left: 2, right: 0 }]);
}

#[test]
fn crossed_moves_keep_the_longer_run() {
    let left = candidates(&[10, 11, 12, 13, 14, 15]);
    let right = candidates(&[10, 11, 15, 12, 13, 14]);
    assert_eq!(
        anchors(&left, &right),
        vec![
            Anchor { left: 0, right: 0 },
            Anchor { left: 1, right: 1 },
            Anchor { left: 2, right: 3 },
            Anchor { left: 3, right: 4 },
            Anchor { left: 4, right: 5 },
        ]
    );
}

#[test]
fn chunk_keys_anchor_by_offset() {
    let left = [
        Candidate {
            key: *b"first chunk_____",
            position: 0,
        },
        Candidate {
            key: *b"second chunk____",
            position: 2_000,
        },
        Candidate {
            key: *b"third chunk_____",
            position: 5_000,
        },
    ];
    let right = [
        Candidate {
            key: *b"second chunk____",
            position: 3_000,
        },
        Candidate {
            key: *b"third chunk_____",
            position: 6_000,
        },
        Candidate {
            key: *b"first chunk_____",
            position: 9_000,
        },
    ];
    assert_eq!(
        anchors(&left, &right),
        vec![
            Anchor {
                left: 2_000,
                right: 3_000,
            },
            Anchor {
                left: 5_000,
                right: 6_000,
            },
        ]
    );
}

#[test]
fn sections_end_at_each_anchor() {
    let left = candidates(&[1, 2, 3, 4]);
    let right = candidates(&[1, 9, 3, 4]);
    let found = anchors(&left, &right);
    assert_eq!(
        found,
        vec![
            Anchor { left: 0, right: 0 },
            Anchor { left: 2, right: 2 },
            Anchor { left: 3, right: 3 },
        ]
    );
    assert_eq!(
        sections(&found, 4, 4),
        vec![
            SectionPair {
                left: 0..1,
                right: 0..1,
            },
            SectionPair {
                left: 1..3,
                right: 1..3,
            },
            SectionPair {
                left: 3..4,
                right: 3..4,
            },
        ]
    );
}

#[test]
fn sections_cover_empty_and_one_sided_ranges() {
    assert_eq!(
        sections(&[], 3, 0),
        vec![SectionPair {
            left: 0..3,
            right: 0..0,
        }]
    );
    assert!(sections(&[], 0, 0).is_empty());
}

#[test]
fn refine_splits_at_anchors_inside_the_section() {
    let section = SectionPair {
        left: 10..20,
        right: 30..40,
    };
    let left = [
        Candidate {
            key: 1_u16,
            position: 12,
        },
        Candidate {
            key: 2_u16,
            position: 16,
        },
    ];
    let right = [
        Candidate {
            key: 1_u16,
            position: 32,
        },
        Candidate {
            key: 2_u16,
            position: 36,
        },
    ];
    assert_eq!(
        refine(&section, &left, &right),
        Some(vec![
            SectionPair {
                left: 10..13,
                right: 30..33,
            },
            SectionPair {
                left: 13..17,
                right: 33..37,
            },
            SectionPair {
                left: 17..20,
                right: 37..40,
            },
        ])
    );
}

#[test]
fn refine_ignores_candidates_outside_the_section() {
    let section = SectionPair {
        left: 10..20,
        right: 30..40,
    };
    let left = candidates(&[1, 2, 3]);
    let right = candidates(&[1, 2, 3]);
    assert_eq!(refine(&section, &left, &right), None);
}

#[test]
fn refine_returns_none_when_in_range_candidates_are_duplicates() {
    let section = SectionPair {
        left: 10..20,
        right: 30..40,
    };
    let left = [
        Candidate {
            key: 7_u16,
            position: 12,
        },
        Candidate {
            key: 7_u16,
            position: 14,
        },
    ];
    let right = [
        Candidate {
            key: 7_u16,
            position: 32,
        },
        Candidate {
            key: 7_u16,
            position: 34,
        },
    ];
    assert_eq!(refine(&section, &left, &right), None);
}

proptest! {
    #[test]
    fn anchors_match_the_lis_oracle_and_cover_both_sides((left, right) in overlapping_sides()) {
        let found = anchors(&positions(&left), &positions(&right));
        prop_assert_eq!(found.len(), oracle_anchor_count(&left, &right));
        for pair in found.windows(2) {
            prop_assert!(pair[0].left < pair[1].left);
            prop_assert!(pair[0].right < pair[1].right);
        }
        for anchor in &found {
            let left_at = usize::try_from(anchor.left).unwrap();
            let right_at = usize::try_from(anchor.right).unwrap();
            prop_assert_eq!(left[left_at], right[right_at]);
            prop_assert_eq!(count_of(&left, left[left_at]), 1);
            prop_assert_eq!(count_of(&right, right[right_at]), 1);
        }
        let left_len = u64::try_from(left.len()).unwrap();
        let right_len = u64::try_from(right.len()).unwrap();
        let parts = sections(&found, left_len, right_len);
        check_cover(&parts, &(0..left_len), &(0..right_len));
    }

    #[test]
    fn unique_lines_without_moves_anchor_only_histogram_equal_lines(
        base in prop::collection::vec(any::<u16>(), 0..40),
        left_mask in any::<u64>(),
        right_mask in any::<u64>(),
        left_added in 0_usize..10,
        right_added in 0_usize..10,
    ) {
        let base = unique(base);
        let left = sequence(&base, left_mask, 100_000, left_added);
        let right = sequence(&base, right_mask, 200_000, right_added);
        let found = anchors(&positions(&left), &positions(&right));
        let equal = histogram_equal(&left, &right);
        for anchor in &found {
            prop_assert!(equal.contains(&(anchor.left, anchor.right)));
        }
    }
}

#[test]
fn crossed_unique_lines_are_not_all_histogram_equal() {
    let left = candidates(&[0, 1, 2, 3]);
    let right = candidates(&[3, 1, 0, 2]);
    let found = anchors(&left, &right);
    assert_eq!(
        found,
        vec![Anchor { left: 1, right: 1 }, Anchor { left: 2, right: 3 }]
    );
    let equal = histogram_equal(&[0, 1, 2, 3], &[3, 1, 0, 2]);
    assert!(
        found
            .iter()
            .all(|anchor| !equal.contains(&(anchor.left, anchor.right)))
    );
}
