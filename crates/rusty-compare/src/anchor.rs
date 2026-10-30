use std::cmp::Ordering;
use std::ops::Range;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Candidate<K> {
    pub key: K,
    /// Line number, chunk index or byte offset within its side.
    pub position: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub left: u64,
    pub right: u64,
}

/// Each anchor line is the last line of its pair; the ranges cover a side once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionPair {
    pub left: Range<u64>,
    pub right: Range<u64>,
}

#[derive(Clone, Copy)]
struct Match {
    left: u64,
    right: u64,
}

#[must_use]
pub fn anchors<K: Ord>(left: &[Candidate<K>], right: &[Candidate<K>]) -> Vec<Anchor> {
    longest_increasing(unique_matches(left, right))
}

#[must_use]
pub fn sections(anchors: &[Anchor], left_len: u64, right_len: u64) -> Vec<SectionPair> {
    cover(anchors, 0..left_len, 0..right_len)
}

/// Re-anchors a section from a denser sample; positions stay absolute.
///
/// Returns `None` when the sample finds no new anchor.
#[must_use]
pub fn refine<K: Copy + Ord>(
    section: &SectionPair,
    left: &[Candidate<K>],
    right: &[Candidate<K>],
) -> Option<Vec<SectionPair>> {
    let left: Vec<Candidate<K>> = left
        .iter()
        .copied()
        .filter(|candidate| section.left.contains(&candidate.position))
        .collect();
    let right: Vec<Candidate<K>> = right
        .iter()
        .copied()
        .filter(|candidate| section.right.contains(&candidate.position))
        .collect();
    let found = anchors(&left, &right);
    if found.is_empty() {
        return None;
    }
    let parts = cover(&found, section.left.clone(), section.right.clone());
    (parts.len() > 1).then_some(parts)
}

fn unique_matches<K: Ord>(left: &[Candidate<K>], right: &[Candidate<K>]) -> Vec<Match> {
    let mut left: Vec<&Candidate<K>> = left.iter().collect();
    let mut right: Vec<&Candidate<K>> = right.iter().collect();
    left.sort_unstable();
    right.sort_unstable();
    let mut matches = Vec::new();
    let mut left_at = 0;
    let mut right_at = 0;
    while let (Some(left_candidate), Some(right_candidate)) =
        (left.get(left_at), right.get(right_at))
    {
        match left_candidate.key.cmp(&right_candidate.key) {
            Ordering::Less => left_at += 1,
            Ordering::Greater => right_at += 1,
            Ordering::Equal => {
                let left_end = run_end(&left, left_at);
                let right_end = run_end(&right, right_at);
                if left_end == left_at + 1 && right_end == right_at + 1 {
                    matches.push(Match {
                        left: left_candidate.position,
                        right: right_candidate.position,
                    });
                }
                left_at = left_end;
                right_at = right_end;
            }
        }
    }
    matches.sort_unstable_by_key(|found| found.left);
    matches
}

fn run_end<K: Ord>(candidates: &[&Candidate<K>], start: usize) -> usize {
    let key = &candidates[start].key;
    let mut end = start + 1;
    while candidates
        .get(end)
        .is_some_and(|candidate| &candidate.key == key)
    {
        end += 1;
    }
    end
}

fn longest_increasing(matches: Vec<Match>) -> Vec<Anchor> {
    let mut tails: Vec<usize> = Vec::new();
    let mut previous = vec![None; matches.len()];
    for (index, found) in matches.iter().enumerate() {
        let at = tails.partition_point(|&tail| matches[tail].right < found.right);
        previous[index] = at
            .checked_sub(1)
            .and_then(|before| tails.get(before))
            .copied();
        match tails.get_mut(at) {
            Some(slot) => *slot = index,
            None => tails.push(index),
        }
    }
    let mut anchors = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(index) = at {
        if let Some(found) = matches.get(index) {
            anchors.push(Anchor {
                left: found.left,
                right: found.right,
            });
        }
        at = previous.get(index).copied().flatten();
    }
    anchors.reverse();
    anchors
}

fn cover(anchors: &[Anchor], left: Range<u64>, right: Range<u64>) -> Vec<SectionPair> {
    let mut parts = Vec::new();
    let mut left_start = left.start;
    let mut right_start = right.start;
    for anchor in anchors {
        let Some(left_end) = anchor.left.checked_add(1) else {
            continue;
        };
        let Some(right_end) = anchor.right.checked_add(1) else {
            continue;
        };
        if anchor.left < left_start || anchor.right < right_start {
            continue;
        }
        if left_end > left.end || right_end > right.end {
            continue;
        }
        parts.push(SectionPair {
            left: left_start..left_end,
            right: right_start..right_end,
        });
        left_start = left_end;
        right_start = right_end;
    }
    if left_start < left.end || right_start < right.end {
        parts.push(SectionPair {
            left: left_start..left.end,
            right: right_start..right.end,
        });
    }
    parts
}

#[cfg(test)]
mod tests;
