use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Subhunk {
    pub old: Vec<String>,
    pub new: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct Candidate {
    distance: usize,
    score: f64,
    old: usize,
    new: usize,
}

#[derive(Debug, Clone)]
pub(super) struct LabelledSubhunk {
    candidates: Vec<Candidate>,
    positives: HashSet<(usize, usize)>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ThresholdMetrics {
    pub threshold: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
}

pub(super) fn pair_rows(subhunk: &Subhunk, threshold: f64) -> Vec<(String, String)> {
    let candidates = candidates(subhunk, threshold);
    greedy(&candidates, threshold)
        .into_iter()
        .map(|(old, new)| (subhunk.old[old].clone(), subhunk.new[new].clone()))
        .collect()
}

pub(super) fn labelled(subhunks: &[Subhunk]) -> Vec<LabelledSubhunk> {
    subhunks
        .iter()
        .map(|subhunk| LabelledSubhunk {
            candidates: candidates(subhunk, 0.8),
            positives: labelled_pairs(subhunk),
        })
        .collect()
}

pub(super) fn metrics(samples: &[LabelledSubhunk], threshold: f64) -> ThresholdMetrics {
    let (mut true_positives, mut false_positives, mut false_negatives) = (0, 0, 0);
    for sample in samples {
        let predicted: HashSet<_> = greedy(&sample.candidates, threshold).into_iter().collect();
        true_positives += predicted.intersection(&sample.positives).count();
        false_positives += predicted.difference(&sample.positives).count();
        false_negatives += sample.positives.difference(&predicted).count();
    }
    let precision = ratio(true_positives, true_positives + false_positives);
    let recall = ratio(true_positives, true_positives + false_negatives);
    let f1 = if precision + recall == 0.0 {
        0.0
    } else {
        2.0 * precision * recall / (precision + recall)
    };
    ThresholdMetrics {
        threshold,
        precision,
        recall,
        f1,
        true_positives,
        false_positives,
        false_negatives,
    }
}

fn candidates(subhunk: &Subhunk, max_score: f64) -> Vec<Candidate> {
    let old: Vec<Vec<char>> = subhunk
        .old
        .iter()
        .map(|line| line.chars().collect())
        .collect();
    let new: Vec<Vec<char>> = subhunk
        .new
        .iter()
        .map(|line| line.chars().collect())
        .collect();
    let mut candidates = Vec::new();
    for (old_index, old_line) in old.iter().enumerate() {
        for (new_index, new_line) in new.iter().enumerate() {
            let (distance, score) = levenshtein_score(old_line, new_line);
            if score <= max_score {
                candidates.push(Candidate {
                    distance,
                    score,
                    old: old_index,
                    new: new_index,
                });
            }
        }
    }
    candidates.sort_by(|a, b| {
        a.distance
            .cmp(&b.distance)
            .then_with(|| a.score.total_cmp(&b.score))
            .then_with(|| a.old.cmp(&b.old))
            .then_with(|| a.new.cmp(&b.new))
    });
    candidates
}

fn greedy(candidates: &[Candidate], threshold: f64) -> Vec<(usize, usize)> {
    let mut used_old = vec![false; candidates.iter().map(|c| c.old + 1).max().unwrap_or(0)];
    let mut used_new = vec![false; candidates.iter().map(|c| c.new + 1).max().unwrap_or(0)];
    let mut pairs = Vec::new();
    for candidate in candidates {
        if candidate.score > threshold || used_old[candidate.old] || used_new[candidate.new] {
            continue;
        }
        used_old[candidate.old] = true;
        used_new[candidate.new] = true;
        pairs.push((candidate.old, candidate.new));
    }
    pairs.sort_unstable();
    pairs
}

fn labelled_pairs(subhunk: &Subhunk) -> HashSet<(usize, usize)> {
    let mut positives = HashSet::new();
    for (old_index, old_line) in subhunk.old.iter().enumerate() {
        for (new_index, new_line) in subhunk.new.iter().enumerate() {
            if word_jaccard(old_line, new_line) >= 0.5 {
                positives.insert((old_index, new_index));
            }
        }
    }
    positives
}

fn word_jaccard(left: &str, right: &str) -> f64 {
    let left = words(left);
    let right = words(right);
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(&right).count();
    let union = left.union(&right).count();
    ratio(intersection, union)
}

fn words(line: &str) -> HashSet<String> {
    line.split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn ratio(part: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 / total as f64
    }
}

fn levenshtein_score(left: &[char], right: &[char]) -> (usize, f64) {
    let max_len = left.len().max(right.len());
    if max_len == 0 {
        return (0, 0.0);
    }
    let mut start = 0;
    while start < left.len() && start < right.len() && left[start] == right[start] {
        start += 1;
    }
    let mut end = 0;
    while end < left.len() - start
        && end < right.len() - start
        && left[left.len() - 1 - end] == right[right.len() - 1 - end]
    {
        end += 1;
    }
    let left = &left[start..left.len() - end];
    let right = &right[start..right.len() - end];
    let distance = levenshtein(left, right);
    (distance, distance as f64 / max_len as f64)
}

fn levenshtein(left: &[char], right: &[char]) -> usize {
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0; right.len() + 1];
    for (left_index, left_char) in left.iter().enumerate() {
        current[0] = left_index + 1;
        for (right_index, right_char) in right.iter().enumerate() {
            let substitution = previous[right_index] + usize::from(left_char != right_char);
            current[right_index + 1] = (previous[right_index + 1] + 1)
                .min(current[right_index] + 1)
                .min(substitution);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subhunk(old: &[&str], new: &[&str]) -> Subhunk {
        Subhunk {
            old: old.iter().map(|line| (*line).to_owned()).collect(),
            new: new.iter().map(|line| (*line).to_owned()).collect(),
        }
    }

    #[test]
    fn identical_and_edited_lines_pair() {
        let sample = subhunk(&["let value = 1;", "unrelated"], &["let value = 2;"]);
        let pairs = pair_rows(&sample, 0.6);
        assert_eq!(
            pairs,
            vec![("let value = 1;".to_owned(), "let value = 2;".to_owned())]
        );
    }

    #[test]
    fn unpaired_lines_stay_above_the_threshold() {
        let sample = subhunk(&["completely different"], &["nothing alike"]);
        assert!(pair_rows(&sample, 0.4).is_empty());
    }

    #[test]
    fn labels_and_metrics_are_bounded() {
        let sample = subhunk(&["alpha beta gamma"], &["alpha beta delta"]);
        let samples = labelled(&[sample]);
        let metrics = metrics(&samples, 0.5);
        assert!((0.0..=1.0).contains(&metrics.precision));
        assert!((0.0..=1.0).contains(&metrics.recall));
        assert!((0.0..=1.0).contains(&metrics.f1));
    }
}
