use std::collections::{BTreeMap, VecDeque};
use std::io::{Read, Seek, SeekFrom};
use std::sync::Arc;

use flate2::{Decompress, FlushDecompress, Status};

use super::header::{ObjectHeader, u16_at, u32_at};
use super::{Error, MAX_CONTAINER, Result, check, count_skipped, read_exact};
use crate::Watch;

#[derive(Debug)]
pub(super) struct Container {
    pub offset: u64,
    pub next: u64,
    pub data: Vec<u8>,
}

#[derive(Default)]
pub(super) struct Cache {
    kept: VecDeque<Arc<Container>>,
    #[cfg(test)]
    pub inflations: usize,
}

impl Cache {
    pub fn find(&mut self, offset: u64) -> Option<Arc<Container>> {
        let at = self.kept.iter().position(|item| item.offset == offset)?;
        let found = self.kept.remove(at)?;
        self.kept.push_back(Arc::clone(&found));
        Some(found)
    }

    pub fn keep(&mut self, item: Arc<Container>) {
        let wanted = item.data.len();
        while !self.kept.is_empty()
            && (self.kept.len() >= 2
                || self.kept.iter().map(|item| item.data.len()).sum::<usize>() + wanted
                    > MAX_CONTAINER)
        {
            self.kept.pop_front();
        }
        self.kept.push_back(item);
    }
}

pub(super) fn load(
    source: &mut (impl Read + Seek),
    (mut at, end): (u64, u64),
    skipped: &mut BTreeMap<u32, u64>,
    watch: &mut Watch<'_>,
) -> Result<Option<Container>> {
    while at < end {
        check(watch, at)?;
        source.seek(SeekFrom::Start(at))?;
        let mut bytes = [0; 16];
        read_exact(source, &mut bytes)?;
        let head = ObjectHeader::parse(&bytes)?;
        let framed = at
            .checked_add(head.size as u64)
            .filter(|framed| *framed <= end)
            .ok_or(Error::Invalid("truncated BLF object or container"))?;
        let next = framed.saturating_add((head.size % 4) as u64).min(end);
        if head.kind == 10 {
            let data = read_container(source, head)?;
            return Ok(Some(Container {
                offset: at,
                next,
                data,
            }));
        }
        count_skipped(skipped, head.kind)?;
        at = next;
    }
    Ok(None)
}

fn read_container(source: &mut (impl Read + Seek), head: ObjectHeader) -> Result<Vec<u8>> {
    source.seek(SeekFrom::Current((head.header_size - 16) as i64))?;
    let mut bytes = [0; 16];
    if head.size < head.header_size + bytes.len() {
        return Err(Error::Invalid("short BLF container header"));
    }
    read_exact(source, &mut bytes)?;
    let size = u32_at(&bytes, 8)? as usize;
    let packed = head.size - head.header_size - bytes.len();
    if size > MAX_CONTAINER || packed > MAX_CONTAINER {
        return Err(Error::Invalid("BLF container exceeds 64 MiB"));
    }
    match u16_at(&bytes, 0)? {
        0 if size == packed => {
            let mut data = vec![0; size];
            read_exact(source, &mut data)?;
            Ok(data)
        }
        0 => Err(Error::Invalid("BLF container size mismatch")),
        2 => inflate(source, packed, size),
        _ => Err(Error::Invalid("unsupported BLF container compression")),
    }
}

fn inflate(source: &mut impl Read, mut packed: usize, size: usize) -> Result<Vec<u8>> {
    let mut decoder = Decompress::new(true);
    let mut output = vec![0; size + 1];
    let mut input = [0; 32 * 1024];
    let (mut start, mut end) = (0, 0);
    loop {
        if start == end && packed > 0 {
            end = input.len().min(packed);
            read_exact(source, &mut input[..end])?;
            packed -= end;
            start = 0;
        }
        let (before_in, before_out) = (decoder.total_in(), decoder.total_out());
        let out =
            usize::try_from(before_out).map_err(|_| Error::Invalid("BLF inflate overflow"))?;
        let status = decoder
            .decompress(
                &input[start..end],
                &mut output[out..],
                FlushDecompress::None,
            )
            .map_err(|_| Error::Invalid("corrupt BLF zlib stream"))?;
        start += (decoder.total_in() - before_in) as usize;
        if decoder.total_out() > size as u64 {
            return Err(Error::Invalid("BLF inflated size mismatch"));
        }
        if status == Status::StreamEnd {
            if decoder.total_out() != size as u64 || start != end || packed != 0 {
                return Err(Error::Invalid("BLF zlib size mismatch"));
            }
            output.truncate(size);
            return Ok(output);
        }
        if decoder.total_in() == before_in && decoder.total_out() == before_out {
            return Err(Error::Invalid("truncated BLF zlib stream"));
        }
    }
}
