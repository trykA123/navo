//! The lockstep reader of two files and its byte counts, used by the content checker.

use std::io::Read;

use crate::model::{ReadHint, Stamp};

use super::checker::Sides;
use super::hash::block_for;
use super::read::{changed, open_pair, probe_zero_size, read_pair, unavailable};
use super::{ContentResult, Side};
#[derive(Clone, Copy)]
pub(super) struct Counts {
    pub(super) done: u64,
    pub(super) total: u64,
}

impl Counts {
    pub(super) const fn new(total: u64) -> Self {
        Self { done: 0, total }
    }

    pub(super) fn add(&mut self, read: usize, block: usize) -> bool {
        self.done = self
            .done
            .saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        self.done > self.total || (read < block && self.done != self.total)
    }
}

pub(super) fn probe_zero_sizes(
    left: Side<'_>,
    right: Side<'_>,
) -> Result<(Option<Stamp>, Option<Stamp>), ContentResult> {
    let left_stamp = probe_zero_size(left).map_err(unavailable)?;
    let right_stamp = probe_zero_size(right).map_err(unavailable)?;
    Ok((left_stamp, right_stamp))
}

pub(super) struct Stream {
    left_reader: Box<dyn Read + Send>,
    right_reader: Box<dyn Read + Send>,
    pub(super) left_buffer: Vec<u8>,
    pub(super) right_buffer: Vec<u8>,
    pub(super) left_counts: Counts,
    right_counts: Counts,
}

pub(super) struct Step {
    pub(super) left: usize,
    pub(super) right: usize,
    pub(super) left_eof: bool,
    pub(super) right_eof: bool,
}

impl Stream {
    pub(super) fn open(sides: Sides<'_>, hint: ReadHint) -> Result<Self, ContentResult> {
        let (left_reader, right_reader) = open_pair(sides.left, sides.right, hint)?;
        let block = block_for(sides.left.entry.size);
        Ok(Self {
            left_reader,
            right_reader,
            left_buffer: vec![0u8; block],
            right_buffer: vec![0u8; block],
            left_counts: Counts::new(sides.left.entry.size),
            right_counts: Counts::new(sides.right.entry.size),
        })
    }

    pub(super) fn step(&mut self) -> Result<Step, ContentResult> {
        let (left, right) = read_pair(
            &mut *self.left_reader,
            &mut *self.right_reader,
            &mut self.left_buffer,
            &mut self.right_buffer,
        )?;
        let left_changed = self.left_counts.add(left, self.left_buffer.len());
        let right_changed = self.right_counts.add(right, self.right_buffer.len());
        if left_changed || right_changed {
            return Err(changed());
        }
        Ok(Step {
            left,
            right,
            left_eof: left < self.left_buffer.len(),
            right_eof: right < self.right_buffer.len(),
        })
    }
}
