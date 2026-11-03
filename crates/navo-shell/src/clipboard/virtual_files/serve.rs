use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::time::Duration;

const CHUNK: usize = 64 * 1024;
const IDLE: Duration = Duration::from_millis(20);
const FULL_WAIT: Duration = Duration::from_millis(5);

/// The bytes of one item, read on the reader thread.
pub trait ItemSource {
    /// Returns 0 at the end of the item.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, String>;
}

/// What a source hands out for one item.
pub enum Opened {
    Stream(Box<dyn ItemSource>),
    /// A compound file (Outlook mail) that only `Backend::write_storage` can save.
    Storage,
}

/// The reader-thread side of a virtual-file source. Every call runs on that one thread.
pub trait Backend {
    fn open(&mut self, index: u32) -> Result<Opened, String>;
    /// Creates `path` (never over an existing file) and saves the storage item into it. Calls
    /// `created` right after the file exists, before the copy starts.
    fn write_storage(
        &mut self,
        index: u32,
        path: &Path,
        created: &mut dyn FnMut(),
    ) -> Result<(), String>;
    /// Runs between requests and while a reader is behind, for message pumping.
    fn idle(&mut self) {}
    /// Runs once when the clipboard handle is dropped; `complete` is false after any failure,
    /// any abandoned item, or when nothing was read.
    fn finish(&mut self, _complete: bool) {}
}

pub(super) enum Chunk {
    /// The source gave a stream: the slow `GetData` is over and reads follow.
    Opened,
    Storage,
    Data(Vec<u8>),
    End,
    Failed(String),
}

pub(super) enum Request {
    Open {
        index: u32,
        chunks: SyncSender<Chunk>,
    },
    Storage {
        index: u32,
        path: PathBuf,
        reply: SyncSender<StorageReply>,
    },
}

pub(super) enum StorageReply {
    /// The target file exists.
    Created,
    Done(Result<(), String>),
}

/// Serves requests until every sender is gone, then calls `finish`.
pub(super) fn serve(backend: &mut dyn Backend, requests: &Receiver<Request>) {
    let mut served = false;
    let mut failed = false;
    loop {
        let ok = match requests.recv_timeout(IDLE) {
            Ok(Request::Open { index, chunks }) => pump(backend, index, &chunks),
            Ok(Request::Storage { index, path, reply }) => store(backend, (index, &path), &reply),
            Err(RecvTimeoutError::Timeout) => {
                backend.idle();
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        };
        served = true;
        failed |= !ok;
    }
    backend.finish(served && !failed);
}

fn pump(backend: &mut dyn Backend, index: u32, chunks: &SyncSender<Chunk>) -> bool {
    let mut source = match backend.open(index) {
        Ok(Opened::Stream(source)) => source,
        Ok(Opened::Storage) => return send(backend, chunks, Chunk::Storage),
        Err(message) => {
            send(backend, chunks, Chunk::Failed(message));
            return false;
        }
    };
    if !send(backend, chunks, Chunk::Opened) {
        return false;
    }
    let mut buf = vec![0; CHUNK];
    loop {
        let chunk = match source.read(&mut buf) {
            Ok(0) => return send(backend, chunks, Chunk::End),
            Ok(read) => Chunk::Data(buf[..read.min(CHUNK)].to_vec()),
            Err(message) => {
                send(backend, chunks, Chunk::Failed(message));
                return false;
            }
        };
        if !send(backend, chunks, chunk) {
            return false;
        }
    }
}

/// False when the reader is gone: the job was cancelled or gave up on a stall.
fn send(backend: &mut dyn Backend, chunks: &SyncSender<Chunk>, mut chunk: Chunk) -> bool {
    loop {
        match chunks.try_send(chunk) {
            Ok(()) => return true,
            Err(TrySendError::Disconnected(_)) => return false,
            Err(TrySendError::Full(back)) => {
                chunk = back;
                backend.idle();
                std::thread::sleep(FULL_WAIT);
            }
        }
    }
}

fn store(
    backend: &mut dyn Backend,
    (index, path): (u32, &Path),
    reply: &SyncSender<StorageReply>,
) -> bool {
    let mut created = || {
        let _ = reply.try_send(StorageReply::Created);
    };
    let written = backend.write_storage(index, path, &mut created);
    let ok = written.is_ok();
    if reply.try_send(StorageReply::Done(written)).is_err() {
        // The caller stopped waiting, so the file it no longer expects must not stay.
        if ok {
            let _ = std::fs::remove_file(path);
        }
        return false;
    }
    ok
}
