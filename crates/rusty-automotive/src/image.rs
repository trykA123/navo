use std::ops::Range;

use crate::record::{Cursor, Event, decode};
use crate::scan::{MAX_SPAN, Piece};
use crate::{Format, ReadAt};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Linear(u64),
    Segment { cs: u16, ip: u16 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    Checksum { line: u64, stored: u8, computed: u8 },
    Malformed { line: u64, reason: &'static str },
    Overlap { line: u64, range: Range<u64> },
    AfterEnd { line: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Range(Range<u64>),
    Gap(Range<u64>),
}

impl Item {
    #[must_use]
    pub fn span(&self) -> &Range<u64> {
        match self {
            Self::Range(span) | Self::Gap(span) => span,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts([u64; 10]);

impl Counts {
    #[must_use]
    pub fn get(&self, kind: u8) -> u64 {
        self.0.get(usize::from(kind)).copied().unwrap_or(0)
    }

    pub(crate) fn add(&mut self, kind: u8) {
        if let Some(count) = self.0.get_mut(usize::from(kind)) {
            *count += 1;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub format: Format,
    pub ranges: Vec<Range<u64>>,
    pub counts: Counts,
    pub start: Option<Start>,
    pub has_end: bool,
    pub problems: Vec<Problem>,
    pub problem_total: u64,
    pub checksum_errors: u64,
    pub lines: u64,
    pub(crate) pieces: Vec<Piece>,
}

impl Image {
    #[must_use]
    pub fn data_len(&self) -> u64 {
        self.ranges
            .iter()
            .map(|range| range.end - range.start)
            .sum()
    }

    #[must_use]
    pub fn gaps(&self) -> Vec<Range<u64>> {
        self.ranges
            .iter()
            .zip(self.ranges.iter().skip(1))
            .map(|(before, after)| before.end..after.start)
            .collect()
    }

    #[must_use]
    pub fn items(&self) -> Vec<Item> {
        let mut items = Vec::with_capacity(self.ranges.len() * 2);
        for (at, range) in self.ranges.iter().enumerate() {
            if let Some(before) = at.checked_sub(1).and_then(|at| self.ranges.get(at)) {
                items.push(Item::Gap(before.end..range.start));
            }
            items.push(Item::Range(range.clone()));
        }
        items
    }

    #[must_use]
    pub fn item_at(&self, address: u64) -> Option<usize> {
        let at = self.ranges.partition_point(|range| range.end <= address);
        let range = self.ranges.get(at)?;
        if range.start <= address {
            Some(at * 2)
        } else {
            (at > 0).then(|| at * 2 - 1)
        }
    }

    pub fn read(
        &self,
        window: Range<u64>,
        read: &mut ReadAt<'_>,
    ) -> Result<Vec<Option<u8>>, String> {
        let len = usize::try_from(window.end.saturating_sub(window.start))
            .map_err(|_| "The window is too large to read.".to_owned())?;
        let mut out = vec![None; len];
        let from = self
            .pieces
            .partition_point(|piece| piece.start.saturating_add(MAX_SPAN) <= window.start);
        let to = self
            .pieces
            .partition_point(|piece| piece.start < window.end);
        let mut hits: Vec<&Piece> = self
            .pieces
            .get(from..to)
            .unwrap_or_default()
            .iter()
            .filter(|piece| piece.end > window.start)
            .collect();
        hits.sort_by_key(|piece| piece.offset);
        for piece in hits {
            fill(self.format, piece, (&window, &mut out), read)?;
        }
        Ok(out)
    }
}

fn fill(
    format: Format,
    piece: &Piece,
    (window, out): (&Range<u64>, &mut [Option<u8>]),
    read: &mut ReadAt<'_>,
) -> Result<(), String> {
    let len = usize::try_from(piece.file_end - piece.offset)
        .map_err(|_| "A record block is too large to read.".to_owned())?;
    let text = read(piece.offset, len)?;
    let mut cursor = Cursor {
        format,
        base: piece.base,
    };
    let mut bytes = Vec::new();
    for line in text.split(|byte| *byte == b'\n').map(<[u8]>::trim_ascii) {
        let Ok(record) = decode(format, line, &mut bytes) else {
            continue;
        };
        if let Ok(Event::Data(address)) = cursor.locate(&record) {
            place(address, record.data, window, out);
        }
    }
    Ok(())
}

fn place(address: u64, data: &[u8], window: &Range<u64>, out: &mut [Option<u8>]) {
    let from = address.max(window.start);
    let to = (address + data.len() as u64).min(window.end);
    for at in from..to {
        let slot = out.get_mut((at - window.start) as usize);
        let byte = data.get((at - address) as usize);
        if let (Some(slot), Some(byte)) = (slot, byte) {
            *slot = Some(*byte);
        }
    }
}
