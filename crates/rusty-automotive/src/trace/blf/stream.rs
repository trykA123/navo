use std::collections::BTreeMap;
use std::io::{Read, Seek};
use std::sync::Arc;

use super::container::{Cache, Container, load};
use super::header::ObjectHeader;
use super::{BlfPosition, Error, Result, count_skipped, frame};
use crate::{Frame, Watch};

pub(super) struct Object {
    pub position: BlfPosition,
    pub frame: Option<(u64, Frame)>,
}

pub(super) struct Stream<R> {
    source: R,
    end: u64,
    next_file: u64,
    current: Option<Arc<Container>>,
    inner: usize,
    pub cache: Cache,
    pub skipped: BTreeMap<u32, u64>,
    pub position: BlfPosition,
}

impl<R: Read + Seek> Stream<R> {
    pub fn new(source: R, end: u64, at: u64) -> Self {
        Self {
            source,
            end,
            next_file: at,
            current: None,
            inner: 0,
            cache: Cache::default(),
            skipped: BTreeMap::new(),
            position: BlfPosition {
                container: at,
                inner: 0,
            },
        }
    }

    pub fn seek(&mut self, at: BlfPosition) {
        self.current = None;
        self.next_file = at.container;
        self.inner = at.inner as usize;
        self.position = at;
    }

    pub fn scanned(&self) -> u64 {
        self.next_file
    }

    fn available(&mut self, watch: &mut Watch<'_>) -> Result<bool> {
        loop {
            if self
                .current
                .as_ref()
                .is_some_and(|current| self.inner < current.data.len())
            {
                return Ok(true);
            }
            if self.current.take().is_some() {
                self.inner = 0;
            }
            let Some(found) = self.load_next(watch)? else {
                return Ok(false);
            };
            if self.inner > found.data.len() {
                return Err(Error::Invalid("BLF checkpoint exceeds its container"));
            }
            self.next_file = found.next;
            self.current = Some(found);
        }
    }

    fn load_next(&mut self, watch: &mut Watch<'_>) -> Result<Option<Arc<Container>>> {
        if let Some(found) = self.cache.find(self.next_file) {
            return Ok(Some(found));
        }
        let Some(found) = load(
            &mut self.source,
            (self.next_file, self.end),
            &mut self.skipped,
            watch,
        )?
        else {
            return Ok(None);
        };
        #[cfg(test)]
        {
            self.cache.inflations += 1;
        }
        let found = Arc::new(found);
        self.cache.keep(Arc::clone(&found));
        Ok(Some(found))
    }

    fn transfer(&mut self, into: &mut [u8], watch: &mut Watch<'_>) -> Result<()> {
        let mut rest = into;
        while !rest.is_empty() {
            if !self.available(watch)? {
                return Err(Error::Invalid("truncated BLF inner object"));
            }
            let current = self
                .current
                .as_ref()
                .ok_or(Error::Invalid("missing BLF container"))?;
            let count = rest.len().min(current.data.len() - self.inner);
            rest[..count].copy_from_slice(&current.data[self.inner..self.inner + count]);
            self.inner += count;
            rest = &mut rest[count..];
        }
        Ok(())
    }

    fn skip(&mut self, mut count: usize, watch: &mut Watch<'_>) -> Result<()> {
        while count > 0 {
            if !self.available(watch)? {
                return Err(Error::Invalid("truncated BLF inner object"));
            }
            let current = self
                .current
                .as_ref()
                .ok_or(Error::Invalid("missing BLF container"))?;
            let take = count.min(current.data.len() - self.inner);
            self.inner += take;
            count -= take;
        }
        Ok(())
    }

    fn start(&mut self, watch: &mut Watch<'_>) -> Result<Option<[u8; 16]>> {
        let mut bytes = [0; 16];
        for padding in 0..=3 {
            self.position = match &self.current {
                Some(current) if self.inner < current.data.len() => BlfPosition {
                    container: current.offset,
                    inner: self.inner as u32,
                },
                _ => BlfPosition {
                    container: self.next_file,
                    inner: 0,
                },
            };
            if !self.available(watch)? {
                return Ok(None);
            }
            let current = self
                .current
                .as_ref()
                .ok_or(Error::Invalid("missing BLF container"))?;
            self.position = BlfPosition {
                container: current.offset,
                inner: self.inner as u32,
            };
            self.transfer(&mut bytes[..1], watch)?;
            if bytes[0] != 0 {
                self.transfer(&mut bytes[1..], watch)?;
                return Ok(Some(bytes));
            }
            if padding == 3 {
                return Err(Error::Invalid("invalid BLF object padding"));
            }
        }
        Ok(None)
    }

    pub fn next(&mut self, watch: &mut Watch<'_>) -> Result<Option<Object>> {
        let Some(base) = self.start(watch)? else {
            return Ok(None);
        };
        let position = self.position;
        let head = ObjectHeader::parse(&base)?;
        let mut header = [0; 40];
        header[..16].copy_from_slice(&base);
        let prefix = head.header_size.min(header.len());
        self.transfer(&mut header[16..prefix], watch)?;
        self.skip(head.header_size - prefix, watch)?;
        let payload_size = head.size - head.header_size;
        let frame = if frame::is_can(head.kind) {
            let time = head.time_ns(&header[..prefix])?;
            let mut payload = [0; 128];
            let take = payload_size.min(payload.len());
            self.transfer(&mut payload[..take], watch)?;
            self.skip(payload_size - take, watch)?;
            Some((time, frame::decode(head, &payload[..take])?))
        } else {
            self.skip(payload_size, watch)?;
            count_skipped(&mut self.skipped, head.kind)?;
            None
        };
        Ok(Some(Object { position, frame }))
    }
}
