use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};

use crate::CancelToken;

use super::{
    ArchiveFormat, CANCELLED, EntryAt, ZipReader, damaged, open as open_archive, sevenz, tarball,
    zipped,
};

const CHUNK: usize = 512 * 1024;
const DEPTH: usize = 4;

pub(crate) type ChunkSender = SyncSender<Result<Option<Vec<u8>>, String>>;

#[derive(Clone, Copy)]
pub(crate) struct StreamRequest<'a> {
    pub at: EntryAt,
    pub header_offset: Option<u64>,
    pub sender: &'a ChunkSender,
    pub cancel: &'a CancelToken,
    pub gone: &'a AtomicBool,
}

pub(crate) struct ZipRead {
    reader: Arc<Mutex<ZipReader>>,
    done: Option<Box<dyn FnOnce() + Send>>,
}

impl ZipRead {
    pub(crate) fn new(reader: Arc<Mutex<ZipReader>>, done: Box<dyn FnOnce() + Send>) -> Self {
        Self {
            reader,
            done: Some(done),
        }
    }
}

impl Drop for ZipRead {
    fn drop(&mut self) {
        if let Some(done) = self.done.take() {
            done();
        }
    }
}

pub(crate) struct ReadRequest {
    pub path: PathBuf,
    pub at: EntryAt,
    pub header_offset: Option<u64>,
    pub zip: Option<ZipRead>,
}

pub(crate) struct EntryStream {
    chunks: Receiver<Result<Option<Vec<u8>>, String>>,
    current: Vec<u8>,
    at: usize,
    failed: bool,
    done: bool,
    gone: Arc<AtomicBool>,
}

impl Read for EntryStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.done {
            return Ok(0);
        }
        if self.failed {
            return Err(io::Error::other("the archive entry stream stopped"));
        }
        loop {
            if self.at < self.current.len() {
                let take = (self.current.len() - self.at).min(buffer.len());
                buffer[..take].copy_from_slice(&self.current[self.at..self.at + take]);
                self.at += take;
                return Ok(take);
            }
            match self.chunks.recv() {
                Ok(Ok(Some(chunk))) => {
                    self.current = chunk;
                    self.at = 0;
                }
                Ok(Ok(None)) => {
                    self.done = true;
                    return Ok(0);
                }
                Ok(Err(message)) => {
                    self.failed = true;
                    return Err(io::Error::other(message));
                }
                Err(_) => {
                    self.failed = true;
                    return Err(io::Error::other(damaged(
                        "the archive entry stream ended without its end marker",
                    )));
                }
            }
        }
    }
}

impl Drop for EntryStream {
    fn drop(&mut self) {
        self.gone.store(true, Ordering::Release);
    }
}

pub(crate) fn open(request: ReadRequest, cancel: CancelToken) -> EntryStream {
    let gone = Arc::new(AtomicBool::new(false));
    let thread_gone = Arc::clone(&gone);
    let (sender, chunks) = sync_channel(DEPTH);
    std::thread::spawn(move || {
        let stream = StreamRequest {
            at: request.at,
            header_offset: request.header_offset,
            sender: &sender,
            cancel: &cancel,
            gone: &thread_gone,
        };
        let result = match request.zip {
            Some(zip) => {
                let mut archive = zip.reader.lock().unwrap_or_else(PoisonError::into_inner);
                let result = zipped::stream_archive(&mut archive, stream);
                drop(archive);
                drop(zip);
                result
            }
            None => stream_path(&request.path, stream),
        };
        let message = match result {
            Ok(()) => Ok(None),
            Err(message) => Err(message),
        };
        let _ = sender.send(message);
    });
    EntryStream {
        chunks,
        current: Vec::new(),
        at: 0,
        failed: false,
        done: false,
        gone,
    }
}

fn stream_path(path: &Path, request: StreamRequest<'_>) -> Result<(), String> {
    let (file, format) = open_archive(path)?;
    let reader = BufReader::new(file);
    match format {
        ArchiveFormat::Zip => zipped::stream(reader, request),
        ArchiveFormat::SevenZip => sevenz::stream(reader, request),
        _ => tarball::stream(format, reader, request),
    }
}

pub(crate) fn pump(
    mut reader: impl Read,
    sender: &ChunkSender,
    cancel: &CancelToken,
    gone: &AtomicBool,
) -> Result<bool, String> {
    let mut buffer = vec![0; CHUNK];
    loop {
        if gone.load(Ordering::Acquire) {
            return Ok(false);
        }
        if cancel.is_cancelled() {
            return Err(CANCELLED.to_owned());
        }
        let read = reader.read(&mut buffer).map_err(damaged)?;
        if read == 0 {
            return Ok(true);
        }
        if sender.send(Ok(Some(buffer[..read].to_vec()))).is_err() {
            return Ok(false);
        }
    }
}

pub(crate) fn drain(
    mut reader: impl Read,
    cancel: &CancelToken,
    gone: &AtomicBool,
) -> Result<(), String> {
    let mut buffer = [0; 64 * 1024];
    loop {
        if gone.load(Ordering::Acquire) {
            return Err(CANCELLED.to_owned());
        }
        if cancel.is_cancelled() {
            return Err(CANCELLED.to_owned());
        }
        let read = reader.read(&mut buffer).map_err(damaged)?;
        if read == 0 {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_disconnect_without_an_end_marker_is_a_damaged_stream() {
        let (sender, chunks) = sync_channel(1);
        drop(sender);
        let mut stream = EntryStream {
            chunks,
            current: Vec::new(),
            at: 0,
            failed: false,
            done: false,
            gone: Arc::new(AtomicBool::new(false)),
        };
        let mut buffer = [0; 8];
        let error = stream.read(&mut buffer).unwrap_err();
        assert!(error.to_string().contains("damaged"), "{error}");
    }

    #[test]
    fn an_explicit_end_marker_is_a_clean_eof() {
        let (sender, chunks) = sync_channel(1);
        sender.send(Ok(None)).unwrap();
        drop(sender);
        let mut stream = EntryStream {
            chunks,
            current: Vec::new(),
            at: 0,
            failed: false,
            done: false,
            gone: Arc::new(AtomicBool::new(false)),
        };
        let mut buffer = [0; 8];
        assert_eq!(stream.read(&mut buffer).unwrap(), 0);
        assert_eq!(stream.read(&mut buffer).unwrap(), 0);
    }

    #[test]
    fn draining_stops_when_the_reader_is_gone() {
        struct PanicReader;
        impl Read for PanicReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                panic!("the reader was read after it was gone");
            }
        }
        let cancel = CancelToken::new();
        let gone = AtomicBool::new(true);
        assert!(drain(PanicReader, &cancel, &gone).is_err());
    }
}
