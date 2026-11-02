use std::io::{self, BufRead, Read};
use std::sync::atomic::AtomicBool;

pub type ReadAt<'a> = dyn FnMut(u64, usize) -> Result<Vec<u8>, String> + 'a;

pub struct Watch<'a> {
    pub cancel: &'a AtomicBool,
    pub progress: &'a mut dyn FnMut(u64),
}

const CHUNK: u64 = 1024 * 1024;

pub(crate) struct RangeSource<'r, 'f> {
    read: &'r mut ReadAt<'f>,
    next: u64,
    end: u64,
    chunk: Vec<u8>,
    at: usize,
    stop: Option<(u64, &'static str)>,
}

impl<'r, 'f> RangeSource<'r, 'f> {
    pub(crate) fn new(read: &'r mut ReadAt<'f>, start: u64, end: u64) -> Self {
        Self {
            read,
            next: start,
            end,
            chunk: Vec::new(),
            at: 0,
            stop: None,
        }
    }

    fn position(&self) -> u64 {
        self.next - (self.chunk.len() - self.at) as u64
    }

    pub(crate) fn limit_from_here(&mut self, bytes: u64, message: &'static str) {
        self.stop = Some((self.position().saturating_add(bytes), message));
    }
}

impl BufRead for RangeSource<'_, '_> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if let Some((stop, message)) = self.stop
            && self.position() > stop
        {
            return Err(io::Error::other(message));
        }
        if self.at >= self.chunk.len() && self.next < self.end {
            let len = usize::try_from(CHUNK.min(self.end - self.next)).unwrap_or(usize::MAX);
            self.chunk = (self.read)(self.next, len).map_err(io::Error::other)?;
            self.at = 0;
            if self.chunk.is_empty() {
                self.end = self.next;
            }
            self.next += self.chunk.len() as u64;
        }
        Ok(self.chunk.get(self.at..).unwrap_or_default())
    }

    fn consume(&mut self, amount: usize) {
        self.at = (self.at + amount).min(self.chunk.len());
    }
}

impl Read for RangeSource<'_, '_> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let available = self.fill_buf()?;
        let count = available.len().min(out.len());
        if let (Some(target), Some(source)) = (out.get_mut(..count), available.get(..count)) {
            target.copy_from_slice(source);
        }
        self.consume(count);
        Ok(count)
    }
}

pub(crate) fn line_column_at(read: &mut ReadAt<'_>, offset: u64) -> (u64, u64) {
    let mut source = RangeSource::new(read, 0, offset);
    let (mut line, mut column) = (1u64, 1u64);
    while let Ok(chunk) = source.fill_buf() {
        if chunk.is_empty() {
            break;
        }
        for byte in chunk {
            if *byte == b'\n' {
                line += 1;
                column = 1;
            } else if byte & 0xC0 != 0x80 {
                column += 1;
            }
        }
        let len = chunk.len();
        source.consume(len);
    }
    (line, column)
}
