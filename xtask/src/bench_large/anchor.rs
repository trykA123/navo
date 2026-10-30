use std::cmp::Ordering;

use super::scan::Candidate;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Anchor {
    pub old: u32,
    pub new: u32,
}

pub struct Anchors {
    pub unique_matches: usize,
    pub anchors: Vec<Anchor>,
    pub tracked_bytes: usize,
}

pub fn find(old: &mut [Candidate], new: &mut [Candidate]) -> Anchors {
    old.sort_unstable();
    new.sort_unstable();
    let mut matches = unique_matches(old, new);
    matches.sort_unstable_by_key(|anchor| anchor.old);
    let anchors = longest_increasing(&matches);
    let tracked = old.len() * 16 + new.len() * 16 + matches.len() * 16 + tracked_lis(&matches);
    Anchors {
        unique_matches: matches.len(),
        anchors,
        tracked_bytes: tracked,
    }
}

fn unique_matches(old: &[Candidate], new: &[Candidate]) -> Vec<Anchor> {
    let mut matches = Vec::new();
    let mut left = 0;
    let mut right = 0;
    while left < old.len() && right < new.len() {
        match old[left].hash.cmp(&new[right].hash) {
            Ordering::Less => left += 1,
            Ordering::Greater => right += 1,
            Ordering::Equal => {
                let old_end = run_end(old, left);
                let new_end = run_end(new, right);
                if old_end - left == 1 && new_end - right == 1 {
                    matches.push(Anchor {
                        old: old[left].line,
                        new: new[right].line,
                    });
                }
                left = old_end;
                right = new_end;
            }
        }
    }
    matches
}

fn run_end(candidates: &[Candidate], start: usize) -> usize {
    let mut end = start + 1;
    while end < candidates.len() && candidates[end].hash == candidates[start].hash {
        end += 1;
    }
    end
}

fn longest_increasing(matches: &[Anchor]) -> Vec<Anchor> {
    let mut tails: Vec<usize> = Vec::new();
    let mut previous = vec![usize::MAX; matches.len()];
    for (index, anchor) in matches.iter().enumerate() {
        let at = tails.partition_point(|&tail| matches[tail].new < anchor.new);
        if let Some(&before) = at.checked_sub(1).and_then(|before| tails.get(before)) {
            previous[index] = before;
        }
        if at == tails.len() {
            tails.push(index);
        } else if let Some(slot) = tails.get_mut(at) {
            *slot = index;
        }
    }
    let mut anchors = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(index) = at {
        if let Some(anchor) = matches.get(index) {
            anchors.push(*anchor);
        }
        at = previous
            .get(index)
            .copied()
            .filter(|value| *value != usize::MAX);
    }
    anchors.reverse();
    anchors
}

fn tracked_lis(matches: &[Anchor]) -> usize {
    matches.len() * 16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(hash: u64, line: u32) -> Candidate {
        Candidate { hash, line }
    }

    #[test]
    fn unique_matches_keep_only_lines_present_once_on_both_sides() {
        let mut old = vec![
            candidate(1, 0),
            candidate(1, 1),
            candidate(2, 2),
            candidate(3, 3),
        ];
        let mut new = vec![
            candidate(1, 0),
            candidate(2, 2),
            candidate(3, 3),
            candidate(3, 4),
        ];
        let anchors = find(&mut old, &mut new);
        assert_eq!(anchors.unique_matches, 1);
        assert_eq!(anchors.anchors, vec![Anchor { old: 2, new: 2 }]);
    }

    #[test]
    fn longest_increasing_skips_crossing_matches() {
        let mut old = vec![
            candidate(1, 0),
            candidate(2, 1),
            candidate(3, 2),
            candidate(4, 3),
        ];
        let mut new = vec![
            candidate(1, 1),
            candidate(2, 0),
            candidate(3, 3),
            candidate(4, 2),
        ];
        let anchors = find(&mut old, &mut new);
        assert_eq!(anchors.anchors.len(), 2);
        assert!(
            anchors
                .anchors
                .windows(2)
                .all(|pair| pair[0].old < pair[1].old)
        );
        assert!(
            anchors
                .anchors
                .windows(2)
                .all(|pair| pair[0].new < pair[1].new)
        );
    }
}
