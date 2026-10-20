use std::collections::HashMap;
use std::io::BufRead;
use std::sync::atomic::Ordering;

use super::asc::{Head, Header, Shape};
use super::line::{MAX_LINE, next_line};
use super::text::Clock;
use super::{CanId, FrameKind, Notation};
use crate::{IndexError, Watch};

pub(crate) const BLOCK: u64 = 32;
pub(crate) const NO_KEY: u32 = u32::MAX;
pub const MAX_IDS: usize = 65_536;
const CHECK_EVERY: u64 = 4096;
pub(crate) const KEPT_PROBLEMS: usize = 1000;
pub(crate) const TOO_LONG: &str = "the line is longer than 4 KiB";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdStat {
    pub channel: u16,
    pub id: CanId,
    pub count: u64,
    pub first_ns: u64,
    pub last_ns: u64,
}

impl IdStat {
    #[must_use]
    pub fn mean_period_ns(&self) -> Option<u64> {
        let gaps = self.count.checked_sub(1).filter(|gaps| *gaps > 0)?;
        Some(self.last_ns.saturating_sub(self.first_ns) / gaps)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Kinds {
    pub can: u64,
    pub fd: u64,
    pub remote: u64,
    pub error: u64,
    pub other: u64,
}

impl Kinds {
    #[must_use]
    pub fn frames(&self) -> u64 {
        self.can + self.fd + self.remote + self.error
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceProblem {
    pub line: u64,
    pub reason: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Block {
    pub first_row: u64,
    pub offset: u64,
    pub line: u64,
    pub clock: Clock,
}

#[derive(Debug, Clone, Default)]
pub struct Trace {
    pub blf: Option<super::blf::BlfInfo>,
    pub date: Option<String>,
    pub internal_events: Option<bool>,
    pub kinds: Kinds,
    pub stats: Vec<IdStat>,
    pub channels: Vec<u16>,
    pub first_ns: Option<u64>,
    pub last_ns: Option<u64>,
    pub malformed: u64,
    pub unlisted: u64,
    pub problems: Vec<TraceProblem>,
    pub lines: u64,
    pub end: u64,
    clock: Clock,
    rows: u64,
    row_base: u64,
    pub(crate) blocks: Vec<Block>,
    pub(crate) binary_blocks: Vec<super::blf::Checkpoint>,
    keys: Vec<u32>,
    lookup: HashMap<(u16, CanId), u32>,
}

pub fn index_trace(mut source: impl BufRead, mut watch: Watch<'_>) -> Result<Trace, IndexError> {
    let mut trace = Trace::default();
    trace.feed(&mut source, (u64::MAX, true), &mut watch)?;
    Ok(trace)
}

impl Trace {
    pub(super) fn new_blf() -> Self {
        Self {
            blf: Some(super::blf::BlfInfo::default()),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn supports_follow(&self) -> bool {
        self.blf.is_none()
    }

    pub(super) fn binary_row(&mut self, now: u64, frame: &super::Frame) {
        self.row(
            Some(now),
            Shape::Frame(Head {
                channel: frame.channel,
                id: frame.id,
                direction: frame.direction,
                kind: frame.kind,
                dlc: frame.dlc,
            }),
        );
    }

    #[must_use]
    pub fn rows(&self) -> u64 {
        self.rows
    }

    #[must_use]
    pub fn notation(&self) -> Notation {
        self.clock.notation
    }

    #[must_use]
    pub fn duration_ns(&self) -> u64 {
        match (self.first_ns, self.last_ns) {
            (Some(first), Some(last)) => last.saturating_sub(first),
            _ => 0,
        }
    }

    #[must_use]
    pub fn key_of(&self, row: u64) -> Option<u32> {
        let at = usize::try_from(row.checked_sub(self.row_base)?).ok()?;
        self.keys.get(at).copied().filter(|key| *key != NO_KEY)
    }

    #[must_use]
    pub fn rows_with(&self, key: u32) -> Vec<u64> {
        self.rows_with_from(key, self.row_base)
    }

    #[must_use]
    pub fn rows_with_from(&self, key: u32, from: u64) -> Vec<u64> {
        let skip = usize::try_from(from.saturating_sub(self.row_base)).unwrap_or(usize::MAX);
        self.keys
            .iter()
            .enumerate()
            .skip(skip)
            .filter(|(_, found)| **found == key)
            .map(|(at, _)| self.row_base + at as u64)
            .collect()
    }

    #[must_use]
    pub fn previous_with(&self, row: u64) -> Option<u64> {
        let key = self.key_of(row)?;
        let at = usize::try_from(row - self.row_base).ok()?;
        let before = self.keys.get(..at)?;
        let found = before.iter().rposition(|other| *other == key)?;
        Some(self.row_base + found as u64)
    }

    #[must_use]
    pub fn resume(&mut self) -> Self {
        if self.lookup.len() != self.stats.len() {
            self.lookup = (0..self.stats.len())
                .filter_map(|at| {
                    let stat = self.stats.get(at)?;
                    Some(((stat.channel, stat.id), u32::try_from(at).ok()?))
                })
                .collect();
        }
        Self {
            blf: self.blf.clone(),
            date: self.date.clone(),
            internal_events: self.internal_events,
            kinds: self.kinds,
            stats: self.stats.clone(),
            channels: self.channels.clone(),
            first_ns: self.first_ns,
            last_ns: self.last_ns,
            malformed: self.malformed,
            unlisted: self.unlisted,
            problems: self.problems.clone(),
            lines: self.lines,
            end: self.end,
            clock: self.clock,
            rows: self.rows,
            row_base: self.rows,
            blocks: Vec::new(),
            binary_blocks: Vec::new(),
            keys: Vec::new(),
            lookup: std::mem::take(&mut self.lookup),
        }
    }

    pub fn append(&mut self, mut tail: Self) {
        if tail.row_base != self.rows {
            return;
        }
        self.blocks.append(&mut tail.blocks);
        self.keys.append(&mut tail.keys);
        let (blocks, keys) = (
            std::mem::take(&mut self.blocks),
            std::mem::take(&mut self.keys),
        );
        let row_base = self.row_base;
        *self = Self {
            row_base,
            blocks,
            keys,
            ..tail
        };
    }

    pub fn read_more(
        &mut self,
        mut source: impl BufRead,
        mut watch: Watch<'_>,
        rows: u64,
    ) -> Result<bool, IndexError> {
        self.feed(&mut source, (rows, false), &mut watch)
    }

    pub fn read_chunk(
        &mut self,
        source: &mut impl BufRead,
        watch: &mut Watch<'_>,
        rows: u64,
    ) -> Result<bool, IndexError> {
        self.feed(source, (rows, false), watch)
    }

    fn feed(
        &mut self,
        source: &mut impl BufRead,
        (budget, keeps_partial): (u64, bool),
        watch: &mut Watch<'_>,
    ) -> Result<bool, IndexError> {
        if !self.supports_follow() {
            return Err(IndexError::Read("BLF traces do not support Follow".into()));
        }
        let stop = self.rows.saturating_add(budget);
        let mut line = Vec::with_capacity(MAX_LINE + 2);
        let mut data = Vec::with_capacity(64);
        while self.rows < stop {
            let taken = next_line(source, &mut line)
                .map_err(|error| IndexError::Read(error.to_string()))?;
            let Some(taken) = taken else {
                return Ok(true);
            };
            if !taken.terminated && !keeps_partial {
                return Ok(true);
            }
            let at = self.end;
            self.end += taken.consumed;
            self.lines += 1;
            if taken.too_long {
                self.note(TOO_LONG);
            } else {
                self.take(at, &line, &mut data);
            }
            self.check(watch)?;
        }
        Ok(false)
    }

    fn check(&self, watch: &mut Watch<'_>) -> Result<(), IndexError> {
        if !self.lines.is_multiple_of(CHECK_EVERY) {
            return Ok(());
        }
        if watch.cancel.load(Ordering::Relaxed) {
            return Err(IndexError::Cancelled);
        }
        (watch.progress)(self.end);
        Ok(())
    }

    fn note(&mut self, reason: &'static str) {
        self.malformed += 1;
        if self.problems.len() < KEPT_PROBLEMS {
            self.problems.push(TraceProblem {
                line: self.lines,
                reason,
            });
        }
    }

    fn take(&mut self, at: u64, line: &[u8], data: &mut Vec<u8>) {
        let (time, shape) = self.clock.parse(line, data);
        match shape {
            Shape::Blank => {}
            Shape::Header(header) => self.header(header),
            shape => {
                self.start_row(at);
                let now = self.clock.tick(time);
                self.row(now, shape);
            }
        }
    }

    fn header(&mut self, header: Header) {
        self.clock.read(&header);
        match header {
            Header::Date(date) => self.date = Some(date),
            Header::Events(logged) => self.internal_events = Some(logged),
            Header::Notation { .. } | Header::Other => {}
        }
    }

    fn start_row(&mut self, at: u64) {
        let opens = self
            .blocks
            .last()
            .is_none_or(|block| self.rows - block.first_row >= BLOCK);
        if opens {
            self.blocks.push(Block {
                first_row: self.rows,
                offset: at,
                line: self.lines - 1,
                clock: self.clock,
            });
        }
    }

    fn row(&mut self, now: Option<u64>, shape: Shape) {
        self.rows += 1;
        if let Some(now) = now {
            self.first_ns.get_or_insert(now);
            self.last_ns = Some(now);
        }
        let key = match shape {
            Shape::Frame(head) => self.frame(head, now.unwrap_or(0)),
            Shape::Malformed(reason) => {
                self.kinds.other += 1;
                self.note(reason);
                NO_KEY
            }
            _ => {
                self.kinds.other += 1;
                NO_KEY
            }
        };
        self.keys.push(key);
    }

    fn frame(&mut self, head: Head, now: u64) -> u32 {
        if let Err(at) = self.channels.binary_search(&head.channel) {
            self.channels.insert(at, head.channel);
        }
        let kinds = &mut self.kinds;
        match head.kind {
            FrameKind::Can => kinds.can += 1,
            FrameKind::Fd { .. } => kinds.fd += 1,
            FrameKind::Remote => kinds.remote += 1,
            FrameKind::Error => kinds.error += 1,
        }
        let Some(id) = head.id else {
            return NO_KEY;
        };
        let key = match self.lookup.get(&(head.channel, id)) {
            Some(key) => *key,
            None if self.stats.len() >= MAX_IDS => {
                self.unlisted += 1;
                return NO_KEY;
            }
            None => self.add_stat(head.channel, id, now),
        };
        if let Some(stat) = self.stats.get_mut(key as usize) {
            stat.count += 1;
            stat.last_ns = now;
        }
        key
    }
}

impl Trace {
    fn add_stat(&mut self, channel: u16, id: CanId, now: u64) -> u32 {
        let key = u32::try_from(self.stats.len()).unwrap_or(NO_KEY);
        self.lookup.insert((channel, id), key);
        self.stats.push(IdStat {
            channel,
            id,
            count: 0,
            first_ns: now,
            last_ns: now,
        });
        key
    }
}
