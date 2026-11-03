use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, sync_channel};
use std::time::{Duration, Instant};

use super::serve::{Backend, Chunk, Request, StorageReply, serve};
use super::{Cancel, SourceKind, VirtualEntry, VirtualError};

/// A read that makes no progress for this long fails the item (D104).
pub const STALL: Duration = Duration::from_secs(60);
/// One blocking COM call that cannot report progress: opening an item (an HGLOBAL arrives whole)
/// or saving a storage (D104).
pub const LONG_STALL: Duration = Duration::from_secs(600);
const POLL: Duration = Duration::from_millis(100);
const QUEUED_CHUNKS: usize = 4;

/// Virtual files offered by the clipboard or a drop. Items are served one at a time by a
/// reader thread that owns the source; dropping this ends that thread.
pub struct VirtualClipboard {
    entries: Vec<VirtualEntry>,
    source_kind: SourceKind,
    archive: Option<PathBuf>,
    requests: Sender<Request>,
    stall: Duration,
    long_stall: Duration,
}

/// The content of one item.
pub enum VirtualContent {
    Stream(VirtualReader),
    /// Save it with [`VirtualClipboard::write_storage`].
    Storage,
}

impl VirtualClipboard {
    pub(super) fn new(
        (entries, source_kind, archive): (Vec<VirtualEntry>, SourceKind, Option<PathBuf>),
        requests: Sender<Request>,
    ) -> Self {
        Self {
            entries,
            source_kind,
            archive,
            requests,
            stall: STALL,
            long_stall: LONG_STALL,
        }
    }

    /// A clipboard over any backend, served by a new plain thread (tests and fakes).
    pub fn from_backend(
        entries: Vec<VirtualEntry>,
        source_kind: SourceKind,
        backend: impl FnOnce() -> Box<dyn Backend> + Send + 'static,
    ) -> Result<Self, VirtualError> {
        let (requests, received) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("navo-virtual-files".into())
            .spawn(move || serve(backend().as_mut(), &received))
            .map_err(|error| VirtualError::Source(error.to_string()))?;
        Ok(Self::new((entries, source_kind, None), requests))
    }

    /// For a clipboard from a .zip folder: the .zip file, when the shell named it.
    #[must_use]
    pub fn with_archive(mut self, archive: PathBuf) -> Self {
        self.archive = Some(archive);
        self
    }

    #[must_use]
    pub fn archive(&self) -> Option<&Path> {
        self.archive.as_deref()
    }

    #[must_use]
    pub fn with_stall(mut self, stall: Duration) -> Self {
        self.stall = stall;
        self
    }

    #[must_use]
    pub fn with_long_stall(mut self, long_stall: Duration) -> Self {
        self.long_stall = long_stall;
        self
    }

    #[must_use]
    pub fn entries(&self) -> &[VirtualEntry] {
        &self.entries
    }

    #[must_use]
    pub fn source_kind(&self) -> SourceKind {
        self.source_kind
    }

    /// Starts reading item `index`. Opening may block in one COM call, so it gets the long
    /// limit; once the source has a stream, each chunk is a sign of life for the short one.
    /// Checks `cancel` between short waits.
    pub fn open(&self, index: u32, cancel: Cancel) -> Result<VirtualContent, VirtualError> {
        let (chunks, received) = sync_channel(QUEUED_CHUNKS);
        self.requests
            .send(Request::Open { index, chunks })
            .map_err(|_| stopped())?;
        let mut reader = VirtualReader {
            chunks: received,
            pending: Vec::new(),
            at: 0,
            ended: false,
            cancel,
            stall: self.stall,
        };
        let mut first = reader.next_chunk(self.long_stall)?;
        if matches!(first, Chunk::Opened) {
            first = reader.next_chunk(self.stall)?;
        }
        match first {
            Chunk::Storage => Ok(VirtualContent::Storage),
            chunk => {
                reader.accept(chunk)?;
                Ok(VirtualContent::Stream(reader))
            }
        }
    }

    /// Saves storage item `index` as a compound file at `temp_path`, which must not exist.
    /// `created` runs here, on the caller's thread, once the file exists; if it fails the wait
    /// ends with that error. After any error the reader thread removes the file once its COM call
    /// returns. The copy is one call with no progress, so it gets the long limit.
    pub fn write_storage(
        &self,
        index: u32,
        (temp_path, created): (&Path, &mut dyn FnMut() -> Result<(), String>),
        cancel: &Cancel,
    ) -> Result<(), VirtualError> {
        let (reply, received) = sync_channel(2);
        let path = temp_path.to_path_buf();
        self.requests
            .send(Request::Storage { index, path, reply })
            .map_err(|_| stopped())?;
        let started = Instant::now();
        loop {
            if cancel() {
                return Err(VirtualError::Cancelled);
            }
            match received.recv_timeout(POLL) {
                Ok(StorageReply::Created) => created().map_err(VirtualError::Source)?,
                Ok(StorageReply::Done(written)) => return written.map_err(VirtualError::Source),
                Err(RecvTimeoutError::Timeout) if started.elapsed() >= self.long_stall => {
                    return Err(VirtualError::Stalled);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err(stopped()),
            }
        }
    }
}

fn stopped() -> VirtualError {
    VirtualError::Source("The virtual file reader stopped".into())
}

/// Bytes of one item. Errors carry a [`VirtualError`]; see [`VirtualError::of`].
pub struct VirtualReader {
    chunks: Receiver<Chunk>,
    pending: Vec<u8>,
    at: usize,
    ended: bool,
    cancel: Cancel,
    stall: Duration,
}

impl VirtualReader {
    fn next_chunk(&mut self, limit: Duration) -> Result<Chunk, VirtualError> {
        let started = Instant::now();
        loop {
            if (self.cancel)() {
                return Err(VirtualError::Cancelled);
            }
            match self.chunks.recv_timeout(POLL) {
                Ok(chunk) => return Ok(chunk),
                Err(RecvTimeoutError::Timeout) if started.elapsed() >= limit => {
                    return Err(VirtualError::Stalled);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Err(stopped()),
            }
        }
    }

    fn accept(&mut self, chunk: Chunk) -> Result<(), VirtualError> {
        match chunk {
            Chunk::Data(bytes) => {
                self.pending = bytes;
                self.at = 0;
            }
            Chunk::Opened => {}
            Chunk::End => self.ended = true,
            Chunk::Failed(message) => return Err(VirtualError::Source(message)),
            Chunk::Storage => return Err(VirtualError::Source("Unexpected storage item".into())),
        }
        Ok(())
    }
}

impl Read for VirtualReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.at >= self.pending.len() && !self.ended && !buf.is_empty() {
            let chunk = self.next_chunk(self.stall).map_err(io::Error::other)?;
            self.accept(chunk).map_err(io::Error::other)?;
        }
        if (self.cancel)() {
            return Err(io::Error::other(VirtualError::Cancelled));
        }
        let left = &self.pending[self.at..];
        let count = left.len().min(buf.len());
        buf[..count].copy_from_slice(&left[..count]);
        self.at += count;
        Ok(count)
    }
}
