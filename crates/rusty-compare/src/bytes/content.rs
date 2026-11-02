use std::io::{self, Read};
use std::ops::Range;

use similar::utils::diff_slices;
use similar::{Algorithm, ChangeTag};

use crate::anchor::{self, Anchor};
use crate::model::{CancelToken, DiffStep, ReadHint, Reason};

use super::budget::Reporter;
use super::chunk::{Chunks, chunks};
use super::offset::{self, Job};
use super::{ByteEvent, ByteRegion, ByteSide};

const REFINE_LIMIT: u64 = 64 * 1024;
const SCAN_BLOCK: usize = 64 * 1024;

pub(super) fn run(job: Job<'_>) {
    if job.cancel.is_cancelled() {
        return;
    }
    let total = job.left.entry.size.saturating_add(job.right.entry.size);
    let mut reporter = Reporter::new(job.events, job.cancel, total, job.budget);
    match align(&job, &mut reporter) {
        Ok(false) => {
            drop(reporter);
            if offset::post(job.events, job.cancel, ByteEvent::Step(DiffStep::Coarse)) {
                offset::run(Job {
                    align: super::Align::Offset,
                    ..job
                });
            }
        }
        Ok(true) if !reporter.cancelled() => {
            if reporter.finish() {
                reporter.send(ByteEvent::Done);
            }
        }
        Ok(true) | Err(Reason::Stopped) => {}
        Err(reason) => {
            reporter.send(ByteEvent::Failed(reason));
        }
    }
}

fn align(job: &Job<'_>, reporter: &mut Reporter<'_>) -> Result<bool, Reason> {
    let Some(left) = chunks(open(&job.left)?, reporter, job.budget, 0)? else {
        return Ok(false);
    };
    let Some(right) = chunks(open(&job.right)?, reporter, job.budget, left.capacity())? else {
        return Ok(false);
    };
    emit_aligned(job, reporter, &left, &right)?;
    Ok(true)
}

fn emit_aligned(
    job: &Job<'_>,
    reporter: &mut Reporter<'_>,
    left: &Chunks,
    right: &Chunks,
) -> Result<(), Reason> {
    if left.len != job.left.entry.size || right.len != job.right.entry.size {
        return Err(Reason::ChangedOnDisk);
    }
    if reporter.cancelled() {
        return Err(Reason::Stopped);
    }
    let anchors = anchor::anchors(&left.entries, &right.entries);
    let mut sides = Cursors {
        left: Cursor::new(open(&job.left)?, job.cancel.clone(), left.len),
        right: Cursor::new(open(&job.right)?, job.cancel.clone(), right.len),
    };
    let mut scratch = vec![0u8; SCAN_BLOCK];
    let mut left_at = 0;
    let mut right_at = 0;
    for anchor in anchors {
        let Some(length) = anchor_length(left, right, &anchor) else {
            continue;
        };
        let gap = Gap {
            left: left_at..anchor.left,
            right: right_at..anchor.right,
        };
        emit_gap(&mut sides, gap, reporter, &mut scratch)?;
        left_at = anchor.left.checked_add(length).ok_or(Reason::ReadError)?;
        right_at = anchor.right.checked_add(length).ok_or(Reason::ReadError)?;
    }
    let tail = Gap {
        left: left_at..left.len,
        right: right_at..right.len,
    };
    emit_gap(&mut sides, tail, reporter, &mut scratch)?;
    sides.left.check_end()?;
    sides.right.check_end()
}

fn open(side: &ByteSide) -> Result<Box<dyn Read + Send>, Reason> {
    side.source.open(&side.entry.rel, ReadHint::Buffered)
}

fn anchor_length(left: &Chunks, right: &Chunks, anchor: &Anchor) -> Option<u64> {
    let left_length = left.length_at(anchor.left)?;
    let right_length = right.length_at(anchor.right)?;
    (left_length == right_length).then_some(left_length)
}

struct Gap {
    left: Range<u64>,
    right: Range<u64>,
}
struct Cursors {
    left: Cursor,
    right: Cursor,
}

struct Cursor {
    reader: Box<dyn Read + Send>,
    cancel: CancelToken,
    at: u64,
    len: u64,
}

impl Cursor {
    fn new(reader: Box<dyn Read + Send>, cancel: CancelToken, len: u64) -> Self {
        Self {
            reader,
            cancel,
            at: 0,
            len,
        }
    }

    fn read_into(&mut self, buffer: &mut [u8]) -> Result<(), Reason> {
        let mut filled = 0;
        while filled < buffer.len() {
            if self.cancel.is_cancelled() {
                return Err(Reason::Stopped);
            }
            match self.reader.read(&mut buffer[filled..]) {
                Ok(0) => return Err(Reason::ChangedOnDisk),
                Ok(count) => filled += count,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return Err(Reason::ReadError),
            }
        }
        if self.cancel.is_cancelled() {
            return Err(Reason::Stopped);
        }
        let count = u64::try_from(buffer.len()).map_err(|_| Reason::ReadError)?;
        self.at = self.at.checked_add(count).ok_or(Reason::ReadError)?;
        Ok(())
    }

    fn skip_to(&mut self, target: u64, scratch: &mut [u8]) -> Result<(), Reason> {
        while self.at < target {
            let want = usize::try_from((target - self.at).min(scratch.len() as u64))
                .map_err(|_| Reason::ReadError)?;
            self.read_into(&mut scratch[..want])?;
        }
        Ok(())
    }

    fn check_end(&mut self) -> Result<(), Reason> {
        if self.at != self.len {
            return Err(Reason::ChangedOnDisk);
        }
        let mut probe = [0_u8; 1];
        loop {
            if self.cancel.is_cancelled() {
                return Err(Reason::Stopped);
            }
            match self.reader.read(&mut probe) {
                Ok(0) if self.cancel.is_cancelled() => return Err(Reason::Stopped),
                Ok(0) => return Ok(()),
                Ok(_) if self.cancel.is_cancelled() => return Err(Reason::Stopped),
                Ok(_) => return Err(Reason::ChangedOnDisk),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => return Err(Reason::ReadError),
            }
        }
    }
}

fn emit_gap(
    sides: &mut Cursors,
    gap: Gap,
    reporter: &mut Reporter<'_>,
    scratch: &mut [u8],
) -> Result<(), Reason> {
    sides.left.skip_to(gap.left.start, scratch)?;
    sides.right.skip_to(gap.right.start, scratch)?;
    let left_len = gap.left.end.saturating_sub(gap.left.start);
    let right_len = gap.right.end.saturating_sub(gap.right.start);
    if left_len == 0 && right_len == 0 {
        return Ok(());
    }
    if left_len == 0 || right_len == 0 {
        reporter.region(ByteRegion::single(gap.left.clone(), gap.right.clone()))?;
    } else if left_len <= REFINE_LIMIT && right_len <= REFINE_LIMIT {
        refine(&mut sides.left, &mut sides.right, &gap, reporter)?;
    } else {
        first_difference(&mut sides.left, &mut sides.right, &gap, reporter)?;
    }
    sides.left.skip_to(gap.left.end, scratch)?;
    sides.right.skip_to(gap.right.end, scratch)
}

fn refine(
    left: &mut Cursor,
    right: &mut Cursor,
    gap: &Gap,
    reporter: &mut Reporter<'_>,
) -> Result<(), Reason> {
    let left_len = usize::try_from(gap.left.end - gap.left.start).map_err(|_| Reason::ReadError)?;
    let right_len =
        usize::try_from(gap.right.end - gap.right.start).map_err(|_| Reason::ReadError)?;
    let mut left_bytes = vec![0_u8; left_len];
    let mut right_bytes = vec![0_u8; right_len];
    left.read_into(&mut left_bytes)?;
    right.read_into(&mut right_bytes)?;
    let ops = diff_slices(Algorithm::Myers, &left_bytes, &right_bytes);
    diff_regions(gap.left.start, gap.right.start, ops, reporter)
}

fn diff_regions(
    left_start: u64,
    right_start: u64,
    ops: Vec<(ChangeTag, &[u8])>,
    reporter: &mut Reporter<'_>,
) -> Result<(), Reason> {
    let mut open: Option<(u64, u64, u64, u64)> = None;
    let mut left_at = left_start;
    let mut right_at = right_start;
    for (tag, bytes) in ops {
        let len = u64::try_from(bytes.len()).map_err(|_| Reason::ReadError)?;
        match tag {
            ChangeTag::Equal => {
                close(&mut open, reporter)?;
                left_at = left_at.checked_add(len).ok_or(Reason::ReadError)?;
                right_at = right_at.checked_add(len).ok_or(Reason::ReadError)?;
            }
            ChangeTag::Delete => {
                let entry = open.get_or_insert((left_at, right_at, left_at, right_at));
                left_at = left_at.checked_add(len).ok_or(Reason::ReadError)?;
                entry.2 = left_at;
            }
            ChangeTag::Insert => {
                let entry = open.get_or_insert((left_at, right_at, left_at, right_at));
                right_at = right_at.checked_add(len).ok_or(Reason::ReadError)?;
                entry.3 = right_at;
            }
        }
    }
    close(&mut open, reporter)
}

fn close(
    open: &mut Option<(u64, u64, u64, u64)>,
    reporter: &mut Reporter<'_>,
) -> Result<(), Reason> {
    if let Some((left_start, right_start, left_end, right_end)) = open.take() {
        reporter.region(ByteRegion::single(
            left_start..left_end,
            right_start..right_end,
        ))?;
    }
    Ok(())
}

fn first_difference(
    left: &mut Cursor,
    right: &mut Cursor,
    gap: &Gap,
    reporter: &mut Reporter<'_>,
) -> Result<(), Reason> {
    let left_len = gap.left.end - gap.left.start;
    let right_len = gap.right.end - gap.right.start;
    let common = left_len.min(right_len);
    let block = usize::try_from(common.min(SCAN_BLOCK as u64)).map_err(|_| Reason::ReadError)?;
    let mut left_bytes = vec![0_u8; block];
    let mut right_bytes = vec![0_u8; block];
    let mut done = 0_u64;
    while done < common {
        let want =
            usize::try_from((common - done).min(block as u64)).map_err(|_| Reason::ReadError)?;
        left.read_into(&mut left_bytes[..want])?;
        right.read_into(&mut right_bytes[..want])?;
        if let Some(index) = left_bytes[..want]
            .iter()
            .zip(&right_bytes[..want])
            .position(|(left_byte, right_byte)| left_byte != right_byte)
        {
            let at = done
                .checked_add(u64::try_from(index).map_err(|_| Reason::ReadError)?)
                .ok_or(Reason::ReadError)?;
            let left_at = gap.left.start.checked_add(at).ok_or(Reason::ReadError)?;
            let right_at = gap.right.start.checked_add(at).ok_or(Reason::ReadError)?;
            return reporter.region(ByteRegion::single(
                left_at..gap.left.end,
                right_at..gap.right.end,
            ));
        }
        done += u64::try_from(want).map_err(|_| Reason::ReadError)?;
    }
    match tail_region(left_len, right_len, gap) {
        Some(region) => reporter.region(region),
        None => Ok(()),
    }
}

fn tail_region(left_len: u64, right_len: u64, gap: &Gap) -> Option<ByteRegion> {
    match left_len.cmp(&right_len) {
        std::cmp::Ordering::Greater => Some(ByteRegion::single(
            gap.left.start.checked_add(right_len)?..gap.left.end,
            gap.right.end..gap.right.end,
        )),
        std::cmp::Ordering::Less => Some(ByteRegion::single(
            gap.left.end..gap.left.end,
            gap.right.start.checked_add(left_len)?..gap.right.end,
        )),
        std::cmp::Ordering::Equal => None,
    }
}

#[cfg(test)]
#[path = "tests/content.rs"]
mod tests;
