use std::io::{self, BufWriter, Read, Write};

use super::FileError;
use crate::table::NameTable;
use crate::usn::FrnMap;
use std::collections::TryReserveError;

const CHUNK: usize = 64 * 1024;

/// The snapshot body after the fixed header, written in file order without a full copy in memory.
pub(super) struct Body<'a> {
    pub root: &'a str,
    pub table: &'a NameTable,
    pub ids: &'a FrnMap,
}

impl Body<'_> {
    /// Writes the body and returns how many bytes it wrote.
    pub fn write_to(&self, out: &mut dyn Write) -> io::Result<u64> {
        let mut counted = Counted { out, bytes: 0 };
        let mut buffered = BufWriter::with_capacity(CHUNK, &mut counted);
        self.write_sections(&mut buffered)?;
        buffered.flush()?;
        drop(buffered);
        Ok(counted.bytes)
    }

    fn write_sections(&self, out: &mut impl Write) -> io::Result<()> {
        let (names, offsets, parents, flags) = self.table.raw_parts();
        out.write_all(self.root.as_bytes())?;
        out.write_all(names.as_bytes())?;
        for &offset in offsets {
            out.write_all(&offset.to_le_bytes())?;
        }
        for &parent in parents {
            out.write_all(&parent.to_le_bytes())?;
        }
        for &flag in flags {
            out.write_all(&u16::from(flag).to_le_bytes())?;
        }
        let (sizes, counts) = self.table.stored_totals();
        for &size in sizes {
            out.write_all(&size.to_le_bytes())?;
        }
        for [files, folders] in counts {
            out.write_all(&files.to_le_bytes())?;
            out.write_all(&folders.to_le_bytes())?;
        }
        for (reference, id) in self.ids.iter() {
            out.write_all(&reference.to_le_bytes())?;
            out.write_all(&id.to_le_bytes())?;
        }
        Ok(())
    }
}

struct Counted<'a> {
    out: &'a mut dyn Write,
    bytes: u64,
}

impl Write for Counted<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let written = self.out.write(bytes)?;
        self.bytes += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// Feeds every byte written into the snapshot checksum.
pub(super) struct ChecksumSink(pub crc32fast::Hasher);

impl Write for ChecksumSink {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Reads body sections straight into their final buffers while it checksums them.
pub(super) struct CheckedReader<R> {
    input: R,
    checksum: crc32fast::Hasher,
    chunk: Vec<u8>,
}

impl<R: Read> CheckedReader<R> {
    pub fn new(input: R, prefix: &[u8]) -> Self {
        let mut checksum = crc32fast::Hasher::new();
        checksum.update(prefix);
        Self {
            input,
            checksum,
            chunk: vec![0; CHUNK],
        }
    }

    fn fill(&mut self, buffer: &mut [u8]) -> Result<(), FileError> {
        self.input.read_exact(buffer).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                FileError::BadLength
            } else {
                FileError::Io(error)
            }
        })?;
        self.checksum.update(buffer);
        Ok(())
    }

    pub fn bytes(&mut self, count: u32) -> Result<Vec<u8>, FileError> {
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(count as usize)?;
        bytes.resize(count as usize, 0);
        self.fill(&mut bytes)?;
        Ok(bytes)
    }

    pub fn values<const N: usize, T>(
        &mut self,
        count: u32,
        decode: fn([u8; N]) -> T,
    ) -> Result<Vec<T>, FileError> {
        let mut values = Vec::new();
        values.try_reserve_exact(count as usize)?;
        self.each::<N>(count, |value| {
            values.push(decode(value));
            Ok(())
        })?;
        Ok(values)
    }

    /// Reads the file-reference map; the flag is true when a reference repeats.
    pub fn ids(&mut self, count: u32) -> Result<(FrnMap, bool), FileError> {
        let mut ids = FrnMap::new();
        ids.try_reserve(count as usize)?;
        let mut repeated = false;
        self.each::<20>(count, |[reference @ .., a, b, c, d]| {
            let reference = u128::from_le_bytes(reference);
            repeated |= ids
                .try_insert(reference, u32::from_le_bytes([a, b, c, d]))?
                .is_some();
            Ok(())
        })?;
        Ok((ids, repeated))
    }

    fn each<const N: usize>(
        &mut self,
        count: u32,
        mut take: impl FnMut([u8; N]) -> Result<(), TryReserveError>,
    ) -> Result<(), FileError> {
        let mut chunk = std::mem::take(&mut self.chunk);
        let mut left = count as usize;
        while left > 0 {
            let values = left.min(CHUNK / N);
            let read = &mut chunk[..values * N];
            self.fill(read)?;
            for &value in read.as_chunks::<N>().0 {
                take(value)?;
            }
            left -= values;
        }
        self.chunk = chunk;
        Ok(())
    }

    pub fn checksum(self) -> u32 {
        self.checksum.finalize()
    }
}

#[cfg(test)]
mod tests {
    use super::{CheckedReader, FileError};
    use crate::heap_probe::fail_allocations_from;

    #[test]
    fn a_wide_reference_that_cannot_allocate_is_an_error() {
        let mut entry = [0u8; 20];
        entry[..16].copy_from_slice(&((1u128 << 100) | 8).to_le_bytes());
        let mut reader = CheckedReader::new(&entry[..], &[]);
        // The wide map's first table needs more than the 100-byte budget.
        let _failing = fail_allocations_from(100);
        let result = reader.ids(1);
        assert!(matches!(result, Err(FileError::Allocation(_))));
    }
}
