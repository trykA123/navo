use std::io::{self, BufReader, Read};

use super::asc::{Head, Shape};
use super::index::{Block, Trace};
use super::line::next_line;
use super::{Body, Entry, Frame};
use crate::ReadAt;

const READ_CHUNK: usize = 16 * 1024;

#[derive(Debug, Clone)]
struct Step {
    block: Block,
    end: u64,
    wanted: Vec<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct Plan {
    steps: Vec<Step>,
}

struct Window<'r, 'a> {
    read: &'r mut ReadAt<'a>,
    at: u64,
    end: u64,
}

impl Read for Window<'_, '_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let left = usize::try_from(self.end.saturating_sub(self.at)).unwrap_or(usize::MAX);
        let wanted = buffer.len().min(left);
        if wanted == 0 {
            return Ok(0);
        }
        let bytes = (self.read)(self.at, wanted).map_err(io::Error::other)?;
        let count = bytes.len().min(wanted);
        if let (Some(into), Some(from)) = (buffer.get_mut(..count), bytes.get(..count)) {
            into.copy_from_slice(from);
        }
        self.at += count as u64;
        Ok(count)
    }
}

impl Trace {
    #[must_use]
    pub fn plan(&self, rows: &[u64]) -> Plan {
        let mut rows: Vec<u64> = rows
            .iter()
            .copied()
            .filter(|row| *row < self.rows())
            .collect();
        rows.sort_unstable();
        rows.dedup();
        let mut steps: Vec<Step> = Vec::new();
        for row in rows {
            let at = self.blocks.partition_point(|block| block.first_row <= row);
            let Some(block) = at.checked_sub(1).and_then(|at| self.blocks.get(at)) else {
                continue;
            };
            match steps.last_mut() {
                Some(step) if step.block == *block => step.wanted.push(row),
                _ => steps.push(Step {
                    block: *block,
                    end: self.blocks.get(at).map_or(self.end, |next| next.offset),
                    wanted: vec![row],
                }),
            }
        }
        Plan { steps }
    }
}

fn body(line: &[u8], shape: Shape, (head, data): (Option<Head>, &[u8])) -> Body {
    let text = || String::from_utf8_lossy(line.trim_ascii()).into_owned();
    match (shape, head) {
        (Shape::Malformed(reason), _) => Body::Malformed {
            text: text(),
            reason,
        },
        (_, Some(head)) => Body::Frame(Frame {
            channel: head.channel,
            id: head.id,
            direction: head.direction,
            kind: head.kind,
            dlc: head.dlc,
            data: data.to_vec(),
        }),
        _ => Body::Other(text()),
    }
}

impl Plan {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn decode(&self, read: &mut ReadAt<'_>) -> Result<Vec<Entry>, String> {
        let mut entries = Vec::new();
        for step in &self.steps {
            decode_step(step, read, &mut entries)
                .map_err(|error| format!("Cannot read the trace: {error}"))?;
        }
        Ok(entries)
    }
}

fn decode_step(step: &Step, read: &mut ReadAt<'_>, out: &mut Vec<Entry>) -> io::Result<()> {
    let window = Window {
        read,
        at: step.block.offset,
        end: step.end,
    };
    let mut source = BufReader::with_capacity(READ_CHUNK, window);
    let mut clock = step.block.clock;
    let (mut line_no, mut row) = (step.block.line, step.block.first_row);
    let mut wanted = step.wanted.iter().peekable();
    let (mut line, mut data) = (Vec::new(), Vec::new());
    while let Some(next) = wanted.peek().copied() {
        let Some(taken) = next_line(&mut source, &mut line)? else {
            break;
        };
        line_no += 1;
        if taken.too_long {
            continue;
        }
        let (time, shape) = clock.parse(&line, &mut data);
        let head = match &shape {
            Shape::Blank => continue,
            Shape::Header(header) => {
                clock.read(header);
                continue;
            }
            Shape::Frame(head) => Some(*head),
            Shape::Other | Shape::Malformed(_) => None,
        };
        let now = clock.tick(time);
        if row == *next {
            wanted.next();
            out.push(Entry {
                row,
                line: line_no,
                time_ns: now,
                body: body(&line, shape, (head, &data)),
            });
        }
        row += 1;
    }
    Ok(())
}
