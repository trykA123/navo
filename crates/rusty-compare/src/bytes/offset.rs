use std::io::{self, Read};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::time::Duration;

use crate::model::{CancelToken, ReadHint, Reason};

use super::{Align, ByteEvent, ByteRegion, ByteSide};

pub(super) const BLOCK: usize = 1024 * 1024;
const MERGE_GAP: u64 = 16;

pub(super) struct Job<'a> {
    pub left: ByteSide,
    pub right: ByteSide,
    pub align: Align,
    pub cancel: &'a CancelToken,
    pub events: &'a SyncSender<ByteEvent>,
}

pub(super) fn run(job: Job<'_>) {
    if job.align == Align::Content {
        post(
            job.events,
            job.cancel,
            ByteEvent::Failed(Reason::Unsupported),
        );
        return;
    }
    if job.cancel.is_cancelled() {
        return;
    }
    let left = match open(&job.left) {
        Ok(reader) => reader,
        Err(reason) => {
            post(job.events, job.cancel, ByteEvent::Failed(reason));
            return;
        }
    };
    let right = match open(&job.right) {
        Ok(reader) => reader,
        Err(reason) => {
            post(job.events, job.cancel, ByteEvent::Failed(reason));
            return;
        }
    };
    let mut scanner = Scanner::new(job.events, job.cancel);
    let mut stream = Stream::new(left, right, job.left.entry.size, job.right.entry.size);
    if let Err(reason) = stream.run(job.cancel, &mut scanner) {
        post(job.events, job.cancel, ByteEvent::Failed(reason));
        return;
    }
    if job.cancel.is_cancelled() || scanner.closed {
        return;
    }
    scanner.finish();
    if scanner.closed {
        return;
    }
    if !stream.matches_listed() {
        post(
            job.events,
            job.cancel,
            ByteEvent::Failed(Reason::ChangedOnDisk),
        );
        return;
    }
    if !job.cancel.is_cancelled() {
        scanner.done();
    }
}

fn open(side: &ByteSide) -> Result<Box<dyn Read + Send>, Reason> {
    side.source.open(&side.entry.rel, ReadHint::Buffered)
}

struct Stream {
    left: Box<dyn Read + Send>,
    right: Box<dyn Read + Send>,
    left_buffer: Vec<u8>,
    right_buffer: Vec<u8>,
    left_listed: u64,
    right_listed: u64,
    left_seen: u64,
    right_seen: u64,
    total: u64,
    base: u64,
}

impl Stream {
    fn new(
        left: Box<dyn Read + Send>,
        right: Box<dyn Read + Send>,
        left_listed: u64,
        right_listed: u64,
    ) -> Self {
        Self {
            left,
            right,
            left_buffer: vec![0u8; BLOCK],
            right_buffer: vec![0u8; BLOCK],
            left_listed,
            right_listed,
            left_seen: 0,
            right_seen: 0,
            total: left_listed.max(right_listed),
            base: 0,
        }
    }

    fn run(&mut self, cancel: &CancelToken, scanner: &mut Scanner<'_>) -> Result<(), Reason> {
        loop {
            if cancel.is_cancelled() {
                return Ok(());
            }
            let left = read_block(&mut *self.left, &mut self.left_buffer)
                .map_err(|_| Reason::ReadError)?;
            let right = read_block(&mut *self.right, &mut self.right_buffer)
                .map_err(|_| Reason::ReadError)?;
            self.left_seen = self.left_seen.saturating_add(offset(left));
            self.right_seen = self.right_seen.saturating_add(offset(right));
            scan_block(
                Block {
                    left: &self.left_buffer[..left],
                    right: &self.right_buffer[..right],
                    base: self.base,
                    left_end: self.left_seen,
                    right_end: self.right_seen,
                },
                scanner,
            );
            if scanner.closed {
                return Ok(());
            }
            self.progress(scanner);
            if scanner.closed {
                return Ok(());
            }
            self.base = self.base.saturating_add(offset(BLOCK));
            if left < BLOCK && right < BLOCK {
                return Ok(());
            }
        }
    }

    fn matches_listed(&self) -> bool {
        self.left_seen == self.left_listed && self.right_seen == self.right_listed
    }

    fn progress(&self, scanner: &mut Scanner<'_>) {
        if self.total == 0 {
            return;
        }
        let done = self.total.min(self.left_seen.max(self.right_seen));
        scanner.progress(done, self.total);
    }
}

struct Block<'a> {
    left: &'a [u8],
    right: &'a [u8],
    base: u64,
    left_end: u64,
    right_end: u64,
}

fn scan_block(block: Block<'_>, scanner: &mut Scanner<'_>) {
    let common = block.left.len().min(block.right.len());
    scan_common(
        &block.left[..common],
        &block.right[..common],
        block.base,
        scanner,
    );
    if block.left.len() == block.right.len() || scanner.closed {
        return;
    }
    let start = block.base.saturating_add(offset(common));
    let end = block
        .base
        .saturating_add(offset(block.left.len().max(block.right.len())));
    if block.left.len() > block.right.len() {
        scanner.changed(start, end, block.right_end);
    } else {
        scanner.changed(start, block.left_end, end);
    }
}

fn scan_common(left: &[u8], right: &[u8], base: u64, scanner: &mut Scanner<'_>) {
    let mut at = 0;
    while at < left.len() && !scanner.closed {
        let same = left[at] == right[at];
        let run = if same {
            left[at..]
                .iter()
                .zip(&right[at..])
                .position(|(a, b)| a != b)
        } else {
            left[at..]
                .iter()
                .zip(&right[at..])
                .position(|(a, b)| a == b)
        }
        .unwrap_or(left.len() - at);
        if same {
            scanner.equal(offset(run));
        } else {
            let start = base.saturating_add(offset(at));
            let end = base.saturating_add(offset(at + run));
            scanner.changed(start, end, end);
        }
        at += run;
    }
}

struct OpenRegion {
    start: u64,
    left_end: u64,
    right_end: u64,
    equal: u64,
}

struct Scanner<'a> {
    events: &'a SyncSender<ByteEvent>,
    cancel: &'a CancelToken,
    open: Option<OpenRegion>,
    closed: bool,
}

impl<'a> Scanner<'a> {
    fn new(events: &'a SyncSender<ByteEvent>, cancel: &'a CancelToken) -> Self {
        Self {
            events,
            cancel,
            open: None,
            closed: false,
        }
    }

    fn equal(&mut self, count: u64) {
        let Some(open) = self.open.as_mut() else {
            return;
        };
        open.equal = open.equal.saturating_add(count);
        if open.equal >= MERGE_GAP {
            self.finish_open();
        }
    }

    fn changed(&mut self, start: u64, left_end: u64, right_end: u64) {
        match self.open.as_mut() {
            Some(open) => {
                open.left_end = open.left_end.max(left_end);
                open.right_end = open.right_end.max(right_end);
                open.equal = 0;
            }
            None => {
                self.open = Some(OpenRegion {
                    start,
                    left_end,
                    right_end,
                    equal: 0,
                });
            }
        }
    }

    fn finish(&mut self) {
        self.finish_open();
    }

    fn done(&mut self) {
        if !post(self.events, self.cancel, ByteEvent::Done) {
            self.closed = true;
        }
    }

    fn progress(&mut self, done: u64, total: u64) {
        if !post(
            self.events,
            self.cancel,
            ByteEvent::Progress { done, total },
        ) {
            self.closed = true;
        }
    }

    fn finish_open(&mut self) {
        let Some(open) = self.open.take() else {
            return;
        };
        let region = ByteRegion {
            left: open.start..open.left_end,
            right: open.start..open.right_end,
        };
        if !post(self.events, self.cancel, ByteEvent::Region(region)) {
            self.closed = true;
        }
    }
}

fn post(events: &SyncSender<ByteEvent>, cancel: &CancelToken, event: ByteEvent) -> bool {
    let mut pending = event;
    loop {
        match events.try_send(pending) {
            Ok(()) => return true,
            Err(TrySendError::Disconnected(_)) => return false,
            Err(TrySendError::Full(returned)) => {
                pending = returned;
                if cancel.is_cancelled() {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

fn offset(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn read_block(reader: &mut dyn Read, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(count) => filled += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

#[cfg(test)]
#[path = "tests/offset.rs"]
mod tests;
