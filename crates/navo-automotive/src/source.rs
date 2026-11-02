use std::io::{self, Read, Seek, SeekFrom};

pub struct Source<F> {
    read: F,
    len: u64,
    at: u64,
}

impl<F: FnMut(u64, usize) -> Result<Vec<u8>, String>> Source<F> {
    pub fn new(read: F, len: u64) -> Self {
        Self { read, len, at: 0 }
    }
}

impl<F: FnMut(u64, usize) -> Result<Vec<u8>, String>> Read for Source<F> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let left = usize::try_from(self.len.saturating_sub(self.at)).unwrap_or(usize::MAX);
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

impl<F> Seek for Source<F> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let at = match to {
            SeekFrom::Start(at) => Some(at),
            SeekFrom::End(by) => self.len.checked_add_signed(by),
            SeekFrom::Current(by) => self.at.checked_add_signed(by),
        };
        self.at = at.ok_or_else(|| io::Error::other("seek before the start of the file"))?;
        Ok(self.at)
    }
}
