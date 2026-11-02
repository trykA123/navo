use std::collections::BTreeSet;

use proptest::prelude::*;

use super::{Case, Side, apply_rows, facts_of, row_sets};
use crate::text::apply::ranges;
use crate::text::diff::{Alignment, Row, diff_lines};
use crate::text::normalise::Options;
use crate::text::side::LineEnding;
use crate::text::{Direction, TargetFacts};

enum Piece {
    Raw(Vec<u8>),
    Body(Vec<u8>),
}

struct Reference<'a> {
    left: &'a Side,
    right: &'a Side,
    alignment: &'a Alignment,
    rows: &'a [Vec<Row>],
    picks: &'a BTreeSet<usize>,
    direction: Direction,
    facts: TargetFacts,
}

fn reference(input: &Reference<'_>) -> Vec<u8> {
    let (target, source) = if input.direction == Direction::LeftToRight {
        (input.right, input.left)
    } else {
        (input.left, input.right)
    };
    let mut state = RefState {
        pieces: Vec::new(),
        target,
        source,
    };
    let mut cursor = 0u64;
    for (hunk, rows) in input.alignment.hunks.iter().zip(input.rows) {
        let (target_range, _) = ranges(hunk, input.direction);
        push_raw(
            &mut state.pieces,
            target.bytes[cursor as usize..target_range.start as usize].to_vec(),
        );
        for row in rows {
            push_row(&mut state, input, *row);
        }
        cursor = target_range.end;
    }
    push_raw(&mut state.pieces, target.bytes[cursor as usize..].to_vec());
    render(&state.pieces, input.facts)
}

struct RefState<'a> {
    pieces: Vec<Piece>,
    target: &'a Side,
    source: &'a Side,
}

fn push_row(state: &mut RefState<'_>, input: &Reference<'_>, row: Row) {
    let source_left = input.direction == Direction::LeftToRight;
    let target = state.target;
    let source = state.source;
    match row {
        Row::Changed { left, right } => {
            let (target_index, source_index) = if source_left {
                (right, left)
            } else {
                (left, right)
            };
            if input.picks.contains(&source_index) {
                state.pieces.push(Piece::Body(
                    source.lines[source_index].text.as_bytes().to_vec(),
                ));
            } else {
                push_raw(&mut state.pieces, raw_line(target, target_index));
            }
        }
        Row::Deleted { left } => {
            if source_left {
                if input.picks.contains(&left) {
                    state
                        .pieces
                        .push(Piece::Body(source.lines[left].text.as_bytes().to_vec()));
                }
            } else {
                push_raw(&mut state.pieces, raw_line(target, left));
            }
        }
        Row::Inserted { right } => {
            if source_left {
                push_raw(&mut state.pieces, raw_line(target, right));
            } else if input.picks.contains(&right) {
                state
                    .pieces
                    .push(Piece::Body(source.lines[right].text.as_bytes().to_vec()));
            }
        }
        Row::Equal { .. } => unreachable!("a hunk has no equal rows"),
    }
}

fn push_raw(pieces: &mut Vec<Piece>, bytes: Vec<u8>) {
    if !bytes.is_empty() {
        pieces.push(Piece::Raw(bytes));
    }
}

fn raw_line(side: &Side, index: usize) -> Vec<u8> {
    side.bytes[side.starts[index] as usize..side.starts[index + 1] as usize].to_vec()
}

fn render(pieces: &[Piece], facts: TargetFacts) -> Vec<u8> {
    let dominant = facts.dominant.as_str().as_bytes();
    let mut out = Vec::new();
    let last = pieces.len().saturating_sub(1);
    for (index, piece) in pieces.iter().enumerate() {
        if !out.is_empty()
            && !out.ends_with(b"\n")
            && !matches!(piece, Piece::Raw(bytes) if bytes.is_empty())
        {
            out.extend_from_slice(dominant);
        }
        match piece {
            Piece::Raw(bytes) => out.extend_from_slice(bytes),
            Piece::Body(bytes) => {
                out.extend_from_slice(bytes);
                if index != last || facts.final_newline {
                    out.extend_from_slice(dominant);
                }
            }
        }
    }
    out
}

fn generated_side(bodies: Vec<String>, crlf: Vec<bool>, ended: bool, last_crlf: bool) -> Side {
    let count = bodies.len();
    let mut spec = Vec::with_capacity(count);
    for (index, text) in bodies.into_iter().enumerate() {
        let ending = if index + 1 == count && !ended {
            LineEnding::None
        } else if index + 1 == count {
            if last_crlf {
                LineEnding::CrLf
            } else {
                LineEnding::Lf
            }
        } else if crlf.get(index).copied().unwrap_or(false) {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        };
        spec.push((text, ending));
    }
    Side::new(spec)
}

fn side_strategy() -> impl Strategy<Value = Side> {
    (
        prop::collection::vec("[a-c]{0,4}", 0..8),
        prop::collection::vec(any::<bool>(), 0..8),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(bodies, crlf, ended, last_crlf)| generated_side(bodies, crlf, ended, last_crlf))
}

proptest! {
    #[test]
    fn arbitrary_picks_follow_the_rows(
        left in side_strategy(),
        right in side_strategy(),
        picks in prop::collection::btree_set(0usize..10, 1..=8),
        source_left in any::<bool>(),
    ) {
        let alignment = diff_lines(left.set(), right.set(), &Options::default());
        let rows = row_sets(&alignment);
        let direction = if source_left { Direction::LeftToRight } else { Direction::RightToLeft };
        let target = if source_left { &right } else { &left };
        let facts = facts_of(&target.lines);
        let input = Reference {
            left: &left,
            right: &right,
            alignment: &alignment,
            rows: &rows,
            picks: &picks,
            direction,
            facts,
        };
        let expected = reference(&input);
        let case = Case {
            left: &left,
            right: &right,
            alignment: &alignment,
            rows: &rows,
            picks: &picks,
            direction,
        };
        let got = apply_rows(&case).unwrap();
        prop_assert_eq!(got, expected);
    }
}
