//! The state of the picked lines (V32): which side, which lines, where the keys are, and the
//! copies the picks turn into. Pure: it reads the pair's text and touches no window.

use std::collections::{BTreeMap, BTreeSet};

mod menu;

use super::super::copy::change_of;
use crate::compare_diff::{
    CopyChange, CopyDirection, DiffRow, OnTick, PairRequest, PairText, PickSide, line_on,
};

/// One line that can be picked: its line index, its change and its `DiffRow`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::window_view::compare_screen) struct Spot {
    pub(in crate::window_view::compare_screen) side: PickSide,
    pub(in crate::window_view::compare_screen) line: u32,
    pub(in crate::window_view::compare_screen) hunk: u32,
    pub(in crate::window_view::compare_screen) row: u32,
}

/// A `usize` as `u32`, saturating at the top.
fn small(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// The lines of `side` that can be picked, in order: changed lines of copyable changes.
fn spots(text: &PairText, side: PickSide) -> Vec<Spot> {
    let mut found = Vec::new();
    for (hunk, view) in text.hunks.iter().enumerate() {
        if view.digest.is_none() {
            continue;
        }
        let range = view.rows.start as usize..view.rows.end as usize;
        let rows = text.rows.get(range).unwrap_or_default();
        for (at, row) in rows.iter().enumerate() {
            if let Some(line) = line_on(*row, side) {
                found.push(Spot {
                    side,
                    line,
                    hunk: small(hunk),
                    row: view.rows.start + small(at),
                });
            }
        }
    }
    found
}

/// The place of `line` in `spots`; lines rise with the rows, so it is a binary search.
fn find(spots: &[Spot], line: u32) -> Option<usize> {
    spots.binary_search_by_key(&line, |spot| spot.line).ok()
}

/// The side a copy in `direction` reads from.
fn source_side(direction: CopyDirection) -> PickSide {
    match direction {
        CopyDirection::ToRight => PickSide::Left,
        CopyDirection::ToLeft => PickSide::Right,
    }
}

/// The picked lines of the open pair: which side, which lines, and where the keys are.
#[derive(Default)]
pub(in crate::window_view::compare_screen) struct Pick {
    side: Option<PickSide>,
    pub(super) lines: BTreeSet<u32>,
    /// The picks made before the current Shift range started.
    base: BTreeSet<u32>,
    anchor: Option<u32>,
    cursor: Option<u32>,
}

impl Pick {
    /// Whether any line is picked.
    pub(in crate::window_view::compare_screen) fn has_lines(&self) -> bool {
        !self.lines.is_empty()
    }

    /// Drops every pick and the cursor; returns whether there was anything to drop.
    pub(in crate::window_view::compare_screen) fn clear(&mut self) -> bool {
        let active = self.has_lines() || self.cursor.is_some();
        *self = Self::default();
        active
    }

    /// Picks on `side`; picks on the other side are dropped, as a pick has one side.
    fn begin(&mut self, side: PickSide) {
        if self.side != Some(side) {
            *self = Self {
                side: Some(side),
                ..Self::default()
            };
        }
    }

    /// Makes the picks the base of the next Shift range, with `line` as its start.
    fn rebase(&mut self, line: u32) {
        self.base = self.lines.clone();
        self.anchor = Some(line);
        self.cursor = Some(line);
    }

    /// Picks every line between the anchor and `to`, on top of the earlier picks.
    fn range(&mut self, spots: &[Spot], to: u32) {
        let from = self.anchor.unwrap_or(to);
        let span = from.min(to)..=from.max(to);
        self.lines = self.base.clone();
        self.lines.extend(
            spots
                .iter()
                .map(|spot| spot.line)
                .filter(|line| span.contains(line)),
        );
    }

    /// The line the keys are on, or the first line of change `from` (else the first line).
    fn here(&self, spots: &[Spot], from: Option<usize>) -> Option<usize> {
        let kept = self.cursor.and_then(|line| find(spots, line));
        let first = from.and_then(|hunk| spots.iter().position(|s| s.hunk as usize >= hunk));
        kept.or(first).or(Some(0)).filter(|_| !spots.is_empty())
    }

    /// Shift+↓ or Shift+↑: moves the cursor one picked-able line and picks the range from the
    /// anchor. The first press picks the line the keys start on.
    pub(in crate::window_view::compare_screen) fn step(
        &mut self,
        text: &PairText,
        forward: bool,
        from: Option<usize>,
    ) -> Option<Spot> {
        let side = self.side.unwrap_or(PickSide::Left);
        let spots = spots(text, side);
        let started = self.cursor.and_then(|line| find(&spots, line)).is_some();
        let at = self.here(&spots, from)?;
        let at = match (started, forward) {
            (false, _) => at,
            (true, true) => (at + 1).min(spots.len() - 1),
            (true, false) => at.saturating_sub(1),
        };
        let spot = spots[at];
        if !started {
            self.begin(side);
            self.rebase(spot.line);
        }
        self.cursor = Some(spot.line);
        self.range(&spots, spot.line);
        Some(spot)
    }

    /// Space: ticks or unticks the line the keys are on.
    pub(in crate::window_view::compare_screen) fn toggle(
        &mut self,
        text: &PairText,
        from: Option<usize>,
    ) -> Option<Spot> {
        let side = self.side.unwrap_or(PickSide::Left);
        let spots = spots(text, side);
        let spot = spots[self.here(&spots, from)?];
        self.begin(side);
        self.flip(spot.line);
        Some(spot)
    }

    /// Ticks or unticks `line` and starts a new range there.
    fn flip(&mut self, line: u32) {
        if !self.lines.remove(&line) {
            self.lines.insert(line);
        }
        self.rebase(line);
    }

    /// A click on a tick box; with `shift` it picks the range from the last pick.
    pub(in crate::window_view::compare_screen) fn click(
        &mut self,
        text: &PairText,
        (side, line): (PickSide, u32),
        shift: bool,
    ) -> Option<Spot> {
        let spots = spots(text, side);
        let spot = spots[find(&spots, line)?];
        self.begin(side);
        match (shift, self.anchor) {
            (true, Some(_)) => {
                self.cursor = Some(line);
                self.range(&spots, line);
            }
            _ => self.flip(line),
        }
        Some(spot)
    }

    /// The copies of the picked lines in `direction`, one per change they touch.
    pub(in crate::window_view::compare_screen) fn changes(
        &self,
        text: &PairText,
        pair: &PairRequest,
        direction: CopyDirection,
    ) -> Result<Vec<CopyChange>, &'static str> {
        let from = source_side(direction);
        if self.side != Some(from) {
            return Err("compare-copy-pick-side");
        }
        let mut by_change: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
        for spot in spots(text, from) {
            if self.lines.contains(&spot.line) {
                by_change
                    .entry(spot.hunk)
                    .or_default()
                    .insert(spot.line as usize);
            }
        }
        by_change
            .into_iter()
            .map(|(hunk, lines)| {
                let found = text.hunks.get(hunk as usize);
                change_of(pair, found, direction).map(|change| CopyChange {
                    lines: Some(lines),
                    ..change
                })
            })
            .collect()
    }

    /// The copies that remove the picked lines: whole one-sided changes, copied from the
    /// side that has nothing there. A change that is only partly picked, or that has lines
    /// on both sides, cannot be removed line by line.
    pub(in crate::window_view::compare_screen) fn removal(
        &self,
        text: &PairText,
        pair: &PairRequest,
    ) -> Result<Vec<CopyChange>, &'static str> {
        let side = self.side.filter(|_| self.has_lines());
        let side = side.ok_or("compare-copy-pick")?;
        let direction = match side {
            PickSide::Left => CopyDirection::ToLeft,
            PickSide::Right => CopyDirection::ToRight,
        };
        let touched: BTreeSet<u32> = spots(text, side)
            .iter()
            .filter(|spot| self.lines.contains(&spot.line))
            .map(|spot| spot.hunk)
            .collect();
        let mut changes = Vec::new();
        for hunk in touched {
            let view = text.hunks.get(hunk as usize);
            let rows = view.and_then(|v| text.rows.get(v.rows.start as usize..v.rows.end as usize));
            if !rows.is_some_and(|rows| self.only_picked(rows, side)) {
                return Err("compare-remove-whole");
            }
            changes.push(change_of(pair, view, direction)?);
        }
        Ok(changes)
    }

    /// Whether every changed row of `rows` is a picked line that exists on `side` only.
    fn only_picked(&self, rows: &[DiffRow], side: PickSide) -> bool {
        rows.iter().all(|row| match (row, side) {
            (DiffRow::Equal(..), _) => true,
            (DiffRow::Deleted(line), PickSide::Left)
            | (DiffRow::Inserted(line), PickSide::Right) => self.lines.contains(line),
            _ => false,
        })
    }

    /// What the list needs to draw the tick boxes.
    pub(in crate::window_view::compare_screen) fn draw(&self, on_tick: OnTick) -> PickDraw {
        PickDraw {
            side: self.side,
            lines: self.lines.clone(),
            cursor: self.cursor,
            on_tick,
        }
    }
}

/// The picks as one frame draws them; owned, as the list's row builder outlives the frame.
#[derive(Clone)]
pub(in crate::window_view::compare_screen) struct PickDraw {
    pub(in crate::window_view::compare_screen) side: Option<PickSide>,
    pub(in crate::window_view::compare_screen) lines: BTreeSet<u32>,
    pub(in crate::window_view::compare_screen) cursor: Option<u32>,
    pub(in crate::window_view::compare_screen) on_tick: OnTick,
}
