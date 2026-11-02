use std::cell::Cell;

use navo_core::paging::ByteSource;

use super::{Case, Side, apply_to, facts_of, lines, picks, row_sets, view, whole_hunk};
use crate::text::diff::{Alignment, Hunk, Row};
use crate::text::side::LineEnding;
use crate::text::{ApplyError, Direction, Picked, PickedHunk, apply_hunk, apply_picked};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Event {
    Changed,
    Rewritten,
    Grown,
}

struct Live {
    bytes: Vec<u8>,
    changed: Cell<bool>,
    rewritten: Cell<bool>,
    grown: Cell<u64>,
    reads: Cell<usize>,
    trigger: Option<(usize, Event)>,
}

impl Live {
    fn new(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
            changed: Cell::new(false),
            rewritten: Cell::new(false),
            grown: Cell::new(0),
            reads: Cell::new(0),
            trigger: None,
        }
    }

    fn flagged(bytes: &[u8], event: Event) -> Self {
        let live = Self::new(bytes);
        live.fire(event);
        live
    }

    fn after(bytes: &[u8], reads: usize, event: Event) -> Self {
        Self {
            trigger: Some((reads, event)),
            ..Self::new(bytes)
        }
    }

    fn fire(&self, event: Event) {
        match event {
            Event::Changed => self.changed.set(true),
            Event::Rewritten => self.rewritten.set(true),
            Event::Grown => self.grown.set(7),
        }
    }
}

impl ByteSource for Live {
    fn len(&self) -> u64 {
        u64::try_from(self.bytes.len()).unwrap() + self.grown.get()
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let read = <[u8] as ByteSource>::read_at(&self.bytes, offset, buf)?;
        self.reads.set(self.reads.get() + 1);
        if let Some((after, event)) = self.trigger
            && self.reads.get() == after
        {
            self.fire(event);
        }
        Ok(read)
    }

    fn take_changed(&self) -> bool {
        self.changed.replace(false)
    }

    fn take_rewritten(&self) -> bool {
        self.rewritten.replace(false)
    }
}

fn pair() -> (Side, Side, Hunk) {
    let left = lines(&[
        ("one", LineEnding::Lf),
        ("two", LineEnding::Lf),
        ("three", LineEnding::Lf),
    ]);
    let right = lines(&[
        ("one", LineEnding::Lf),
        ("TWO", LineEnding::Lf),
        ("three", LineEnding::Lf),
    ]);
    let hunk = whole_hunk(1..2, &left.starts, 1..2, &right.starts);
    (left, right, hunk)
}

fn copy(
    target: &dyn ByteSource,
    source: &dyn ByteSource,
    out: &mut Vec<u8>,
) -> Result<(), ApplyError> {
    let (_, right, hunk) = pair();
    apply_hunk(
        target,
        source,
        &hunk,
        Direction::LeftToRight,
        facts_of(&right.lines),
        out,
    )
}

fn refused_before_writing(target: &dyn ByteSource, source: &dyn ByteSource) {
    let mut out = Vec::new();
    let result = copy(target, source, &mut out);
    assert!(matches!(result, Err(ApplyError::Changed)), "{result:?}");
    assert!(out.is_empty());
}

#[test]
fn a_source_edited_after_the_diff_with_the_same_length_is_refused() {
    let (_, right, _) = pair();
    refused_before_writing(
        &view(&right.bytes),
        &Live::flagged(b"one\nTWX\nthree\n", Event::Changed),
    );
}

#[test]
fn a_source_rewritten_after_the_diff_is_refused() {
    let (left, right, _) = pair();
    refused_before_writing(
        &view(&right.bytes),
        &Live::flagged(&left.bytes, Event::Rewritten),
    );
}

#[test]
fn a_source_whose_length_changed_after_the_diff_is_refused() {
    let (left, right, _) = pair();
    let source = Live::flagged(&left.bytes, Event::Grown);
    source.changed.set(true);
    refused_before_writing(&view(&right.bytes), &source);
}

#[test]
fn a_target_changed_before_the_copy_is_refused() {
    let (left, right, _) = pair();
    refused_before_writing(
        &Live::flagged(&right.bytes, Event::Changed),
        &view(&left.bytes),
    );
}

#[test]
fn a_source_changed_during_the_copy_is_refused() {
    let (left, right, _) = pair();
    for event in [Event::Changed, Event::Rewritten, Event::Grown] {
        let source = Live::after(&left.bytes, 1, event);
        let result = copy(&view(&right.bytes), &source, &mut Vec::new());
        assert!(source.reads.get() >= 1);
        assert!(matches!(result, Err(ApplyError::Changed)), "{result:?}");
    }
}

#[test]
fn a_target_changed_during_the_copy_is_refused() {
    let (left, right, _) = pair();
    for event in [Event::Changed, Event::Rewritten, Event::Grown] {
        let target = Live::after(&right.bytes, 1, event);
        let result = copy(&target, &view(&left.bytes), &mut Vec::new());
        assert!(matches!(result, Err(ApplyError::Changed)), "{result:?}");
    }
}

#[test]
fn an_unchanged_live_source_still_copies() {
    let (left, right, _) = pair();
    let source = Live::new(&left.bytes);
    let mut out = Vec::new();
    copy(&Live::new(&right.bytes), &source, &mut out).unwrap();
    assert_eq!(out, left.bytes);
    assert!(source.reads.get() >= 1);
}

#[test]
fn picked_lines_from_a_changed_source_are_refused() {
    let (left, right, hunk) = pair();
    let alignment = Alignment {
        rows: vec![
            Row::Equal { left: 0, right: 0 },
            Row::Changed { left: 1, right: 1 },
            Row::Equal { left: 2, right: 2 },
        ],
        hunks: vec![hunk],
        raw_equal: false,
        display_equal: false,
        fallback: None,
    };
    let rows = row_sets(&alignment);
    let chosen = picks([1]);
    let case = Case {
        left: &left,
        right: &right,
        alignment: &alignment,
        rows: &rows,
        picks: &chosen,
        direction: Direction::LeftToRight,
    };
    let mut expected = Vec::new();
    apply_to(&case, &mut expected).unwrap();
    assert_eq!(expected, left.bytes);
    let entries = [PickedHunk {
        hunk: &alignment.hunks[0],
        rows: &rows[0],
    }];
    let picked = Picked {
        facts: facts_of(&right.lines),
        hunks: &entries,
        left_starts: &left.starts,
        right_starts: &right.starts,
        lines: &chosen,
        direction: Direction::LeftToRight,
    };
    let mut out = Vec::new();
    let flagged = Live::flagged(&left.bytes, Event::Changed);
    let result = apply_picked(&view(&right.bytes), &flagged, picked, &mut out);
    assert!(matches!(result, Err(ApplyError::Changed)), "{result:?}");
    assert!(out.is_empty());
    let during = Live::after(&left.bytes, 1, Event::Rewritten);
    let result = apply_picked(&view(&right.bytes), &during, picked, &mut Vec::new());
    assert!(matches!(result, Err(ApplyError::Changed)), "{result:?}");
}
