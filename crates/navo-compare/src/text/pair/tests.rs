use std::time::{Duration, Instant};

use super::*;
use crate::text::side::LineEnding;

fn line(text: &str) -> Line {
    Line {
        text: text.to_owned(),
        ending: LineEnding::None,
        bom: false,
    }
}

fn subhunk(left: &[&str], right: &[&str]) -> (Vec<Line>, Vec<Line>) {
    (
        left.iter().map(|text| line(text)).collect(),
        right.iter().map(|text| line(text)).collect(),
    )
}

fn pairs(left: &[&str], right: &[&str]) -> Pairs {
    let (left, right) = subhunk(left, right);
    pair_lines(Subhunk {
        left: &left,
        right: &right,
    })
}

#[test]
fn a_renamed_variable_pairs_with_its_line_not_its_neighbour() {
    assert_eq!(
        pairs(
            &["let alpha = 1;", "let beta = 2;"],
            &["let alpha = 10;", "let beta = 20;"]
        ),
        vec![(0, 0), (1, 1)]
    );
}

#[test]
fn crossing_candidates_do_not_both_pair() {
    assert_eq!(
        pairs(
            &["let alpha = 1;", "let beta = 2;"],
            &["let beta = 20;", "let alpha = 10;"]
        ),
        vec![(0, 1)]
    );
}

#[test]
fn a_line_without_a_similar_partner_stays_unpaired() {
    assert!(pairs(&["completely different"], &["nothing alike"]).is_empty());
}

#[test]
fn the_sixty_percent_boundary_is_inclusive() {
    assert_eq!(pairs(&["abcde"], &["abxyz"]), vec![(0, 0)]);
    assert!(pairs(&["abcde"], &["vwxyz"]).is_empty());
}

#[test]
fn a_long_subhunk_pairs_in_order() {
    let left: Vec<String> = (0..201)
        .map(|index| format!("left line {index:03}"))
        .collect();
    let right: Vec<String> = (0..201)
        .map(|index| format!("right line {:03}", 200 - index))
        .collect();
    let left: Vec<Line> = left.iter().map(|text| line(text)).collect();
    let right: Vec<Line> = right.iter().map(|text| line(text)).collect();
    let selected = pair_lines(Subhunk {
        left: &left,
        right: &right,
    });

    assert_eq!(selected.len(), 201);
    assert!(selected.iter().all(|(left, right)| left == right));
}

#[test]
fn an_overlong_line_stays_unpaired() {
    let long = "x".repeat(PAIR_MAX_LINE_CHARS + 1);
    assert!(pairs(&[long.as_str()], &["short line"]).is_empty());
}

#[test]
fn the_bit_parallel_distance_matches_the_table() {
    let mut state = 0x1234_5678_9abc_def0u64;
    for length in 0..=130usize {
        let other_length = (length * 3 + 7) % 131;
        for _ in 0..4 {
            let left: Vec<char> = (0..length)
                .map(|_| {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    char::from(b'a' + (state % 5) as u8)
                })
                .collect();
            let right: Vec<char> = (0..other_length)
                .map(|_| {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    char::from(b'a' + (state % 5) as u8)
                })
                .collect();
            let left_line = line(&left.iter().collect::<String>());
            let right_line = line(&right.iter().collect::<String>());
            let got = PreparedLine::new(&left_line)
                .distance(&PreparedLine::new(&right_line))
                .expect("under the work cap");
            assert_eq!(
                got,
                naive(&left, &right),
                "lengths {length} and {other_length}"
            );
        }
    }
}

fn naive(left: &[char], right: &[char]) -> usize {
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

#[test]
#[ignore = "release-only: 200-line pairing must stay under 20 ms"]
#[allow(
    clippy::print_stderr,
    reason = "the ignored perf test reports its measurement"
)]
fn perf_200_line_pairing_under_20_ms() {
    let left: Vec<Line> = (0..200)
        .map(|index| {
            line(&format!(
                "let value_{index:03} = compute_{:02}(input_{:02});",
                index % 17,
                index % 7
            ))
        })
        .collect();
    let right: Vec<Line> = left
        .iter()
        .enumerate()
        .map(|(index, source)| {
            if index % 10 == 0 {
                line(&format!("{} // edited", source.text))
            } else {
                source.clone()
            }
        })
        .collect();
    let mut samples = Vec::new();
    for _ in 0..3 {
        let started = Instant::now();
        let selected = pair_lines(Subhunk {
            left: &left,
            right: &right,
        });
        samples.push(started.elapsed());
        assert_eq!(selected.len(), 200);
    }
    samples.sort_unstable();
    let median = samples[1];
    eprintln!(
        "200-line release pairing: {:.3} ms",
        median.as_secs_f64() * 1_000.0
    );
    assert!(
        median < Duration::from_millis(20),
        "200-line pairing took {median:?}"
    );
}
