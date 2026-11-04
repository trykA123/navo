use std::fs::{self, DirEntry, File, Metadata, ReadDir};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::{
    EntryKind, ReadOnlySource, Record, RecordId, StopReason, WalkConfig, WalkOutcome, WalkSummary,
    platform,
};

pub(super) trait WalkIo {
    fn elapsed(&mut self) -> Duration;
    fn metadata(&mut self, entry: &DirEntry) -> io::Result<Metadata>;
}

struct SystemIo {
    start: Instant,
}

impl WalkIo for SystemIo {
    fn elapsed(&mut self) -> Duration {
        self.start.elapsed()
    }

    fn metadata(&mut self, entry: &DirEntry) -> io::Result<Metadata> {
        platform::entry_metadata(entry)
    }
}

struct Frame {
    _directory: File,
    entries: ReadDir,
    parent: Option<RecordId>,
    depth: usize,
}

impl Frame {
    fn open(path: &Path, parent: Option<RecordId>, depth: usize) -> io::Result<Self> {
        let directory = platform::open_directory(path)?;
        Self::from_directory(directory, path, parent, depth)
    }

    fn from_directory(
        directory: File,
        path: &Path,
        parent: Option<RecordId>,
        depth: usize,
    ) -> io::Result<Self> {
        let entries = fs::read_dir(enumeration_path(path, &directory))?;
        Ok(Self {
            _directory: directory,
            entries,
            parent,
            depth,
        })
    }
}

#[cfg(target_os = "linux")]
fn enumeration_path(_path: &Path, directory: &File) -> PathBuf {
    use std::os::fd::AsRawFd;
    PathBuf::from(format!("/proc/self/fd/{}", directory.as_raw_fd()))
}

#[cfg(not(target_os = "linux"))]
fn enumeration_path(path: &Path, _directory: &File) -> PathBuf {
    path.to_path_buf()
}

/// Extra time the worker gets past `max_time` before it is abandoned.
pub(super) const GRACE: Duration = Duration::from_secs(5);
const QUEUE: usize = 1024;

enum Message {
    Record(Record),
    Done(WalkSummary),
}

pub(super) fn run(
    source: &ReadOnlySource,
    config: WalkConfig,
    visit: impl FnMut(Record),
) -> WalkSummary {
    let io = SystemIo {
        start: Instant::now(),
    };
    bounded(source, config, (GRACE, io), visit)
}

/// Walks on a worker and stops waiting at `max_time + grace`; a stuck worker is abandoned.
pub(super) fn bounded<I: WalkIo + Send + 'static>(
    source: &ReadOnlySource,
    config: WalkConfig,
    (grace, mut io): (Duration, I),
    mut visit: impl FnMut(Record),
) -> WalkSummary {
    let failed = WalkSummary {
        files: 0,
        errors: 1,
        outcome: WalkOutcome::Complete,
    };
    let Ok(worker) = source.share() else {
        return failed;
    };
    let (sender, receiver) = mpsc::sync_channel(QUEUE);
    let deadline = Instant::now().checked_add(config.max_time.saturating_add(grace));
    let spawned = std::thread::Builder::new()
        .name("navo-bench-walk".into())
        .spawn(move || {
            let send = |record| sender.send(Message::Record(record)).is_ok();
            let summary = run_with(&worker, config, send, &mut io);
            let _ = sender.send(Message::Done(summary));
        });
    if spawned.is_err() {
        return failed;
    }
    let mut files = 0_u64;
    loop {
        let message = match deadline {
            Some(at) => receiver.recv_timeout(at.saturating_duration_since(Instant::now())),
            None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match message {
            Ok(Message::Record(record)) => {
                files += 1;
                visit(record);
            }
            Ok(Message::Done(summary)) => return summary,
            Err(RecvTimeoutError::Timeout) => return timed_out(files),
            Err(RecvTimeoutError::Disconnected) => return WalkSummary { files, ..failed },
        }
    }
}

fn timed_out(files: u64) -> WalkSummary {
    WalkSummary {
        files,
        errors: 0,
        outcome: WalkOutcome::StoppedAt {
            files,
            reason: StopReason::Timeout,
        },
    }
}

/// Synchronous walk; `visit` returns false to stop it.
pub(super) fn run_with(
    source: &ReadOnlySource,
    config: WalkConfig,
    mut visit: impl FnMut(Record) -> bool,
    io: &mut impl WalkIo,
) -> WalkSummary {
    let mut summary = WalkSummary {
        files: 0,
        errors: 0,
        outcome: WalkOutcome::Complete,
    };
    if stop(&mut summary, config, io) {
        return summary;
    }
    let frame = match source
        ._directory
        .try_clone()
        .and_then(|directory| Frame::from_directory(directory, &source.root, None, 0))
    {
        Ok(frame) => frame,
        Err(_) => {
            summary.errors = 1;
            return summary;
        }
    };
    let mut traversal = Traversal {
        stack: vec![frame],
        summary,
        config,
        cancelled: false,
    };
    while !traversal.stack.is_empty() && !traversal.cancelled {
        if stop(&mut traversal.summary, config, io) {
            break;
        }
        traversal.advance(source, &mut visit, io);
    }
    traversal.summary
}

struct Traversal {
    stack: Vec<Frame>,
    summary: WalkSummary,
    config: WalkConfig,
    cancelled: bool,
}

impl Traversal {
    fn advance(
        &mut self,
        source: &ReadOnlySource,
        visit: &mut impl FnMut(Record) -> bool,
        io: &mut impl WalkIo,
    ) {
        let Some(frame) = self.stack.last_mut() else {
            return;
        };
        let Some(entry) = frame.entries.next() else {
            self.stack.pop();
            return;
        };
        self.summary.files += 1;
        let record = entry.and_then(|entry| make_record(source, frame, &entry, io));
        let (record, scan_path) = match record {
            Ok(record) => record,
            Err(_) => {
                self.summary.errors = self.summary.errors.saturating_add(1);
                return;
            }
        };
        let child = if record.kind == EntryKind::Directory {
            Some((record.id.clone(), record.depth.saturating_add(1), scan_path))
        } else {
            None
        };
        if !visit(record) {
            self.cancelled = true;
            return;
        }
        if let Some((id, depth, path)) = child {
            if stop(&mut self.summary, self.config, io) {
                return;
            }
            match Frame::open(&path, Some(id), depth) {
                Ok(frame) => self.stack.push(frame),
                Err(_) => self.summary.errors = self.summary.errors.saturating_add(1),
            }
        }
    }
}

fn stop(summary: &mut WalkSummary, config: WalkConfig, io: &mut impl WalkIo) -> bool {
    let reason = if io.elapsed() >= config.max_time {
        Some(StopReason::Time)
    } else if summary.files >= config.max_files {
        Some(StopReason::FileCount)
    } else {
        None
    };
    if let Some(reason) = reason {
        summary.outcome = WalkOutcome::StoppedAt {
            files: summary.files,
            reason,
        };
        true
    } else {
        false
    }
}

fn make_record(
    source: &ReadOnlySource,
    frame: &Frame,
    entry: &DirEntry,
    io: &mut impl WalkIo,
) -> io::Result<(Record, PathBuf)> {
    let metadata = io.metadata(entry)?;
    let parent_path = frame
        .parent
        .as_ref()
        .map_or(source.root.as_path(), |id| id.path.as_path());
    let path = parent_path.join(entry.file_name());
    let name = entry.file_name();
    let name_text = name.to_string_lossy();
    let (name_chars, dot_name) = (name_text.chars().count(), name_text.starts_with('.'));
    let git_marker = name_text == ".git";
    let path_chars = path.to_string_lossy().chars().count();
    let extension = path
        .extension()
        .map(|text| text.to_string_lossy().into_owned())
        .unwrap_or_default();
    let record = Record {
        id: RecordId {
            path: Arc::new(path),
            owner: source.owner.clone(),
        },
        parent: frame.parent.clone(),
        depth: frame.depth,
        kind: platform::kind(&metadata, &entry.path()),
        size: metadata.len(),
        attributes: platform::attributes(&metadata),
        mtime: metadata.modified().ok(),
        extension,
        hard_links: platform::hard_links(&metadata),
        name_chars,
        path_chars,
        dot_name,
        git_marker,
    };
    Ok((record, entry.path()))
}
