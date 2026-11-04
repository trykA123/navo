use std::io::{self, Read};
use std::path::Path;
use std::sync::Arc;

use navo_core::{Conflict, Location, ProviderId, Resolution, Totals};

use super::*;
use crate::LocalFs;

const SIZE: usize = 300 * 1024;
const CHUNK: usize = 64 * 1024;
const OLD: &[u8] = b"old bytes";

#[derive(Clone, Copy)]
enum Feed {
    Whole,
    Short(usize),
    Long(usize),
    BreaksAt(usize),
    CancelsAt(usize),
    InterruptsAt(usize),
    MetadataError,
    OpenError,
}

struct Memory {
    bytes: Vec<u8>,
    feed: Feed,
    cancel: CancelToken,
}

struct Chunks {
    bytes: Vec<u8>,
    at: usize,
    breaks_at: Option<usize>,
    cancels: Option<(usize, CancelToken)>,
    interrupts_at: Option<usize>,
}

impl Read for Chunks {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.interrupts_at.is_some_and(|at| self.at >= at) {
            self.interrupts_at = None;
            return Err(io::ErrorKind::Interrupted.into());
        }
        if self.breaks_at.is_some_and(|at| self.at >= at) {
            return Err(io::Error::other("source broke"));
        }
        let end = self.bytes.len().min(self.at + CHUNK.min(buf.len()));
        let read = end - self.at;
        buf[..read].copy_from_slice(&self.bytes[self.at..end]);
        self.at = end;
        if let Some((at, cancel)) = &self.cancels
            && self.at >= *at
        {
            cancel.cancel();
        }
        Ok(read)
    }
}

fn memory_id() -> ProviderId {
    ProviderId::new("mem").unwrap()
}

impl SourceOps for Memory {
    fn id(&self) -> ProviderId {
        memory_id()
    }

    fn metadata(&self, location: &Location) -> Result<SourceMetadata, FsError> {
        if matches!(self.feed, Feed::MetadataError) {
            return Err(super::super::copy::failure(
                FsErrorKind::AccessDenied,
                location,
            ));
        }
        Ok(FsEntry {
            name: location.name().unwrap(),
            kind: EntryKind::File,
            size: self.bytes.len() as u64,
            modified_ms: Some(1_000),
            attributes: Attributes::default(),
        }
        .into())
    }

    fn open_read(&self, location: &Location) -> Result<Box<dyn Read + Send>, FsError> {
        if matches!(self.feed, Feed::OpenError) {
            return Err(super::super::copy::failure(FsErrorKind::Locked, location));
        }
        let mut bytes = self.bytes.clone();
        let breaks_at = match self.feed {
            Feed::Short(by) => {
                bytes.truncate(bytes.len() - by);
                None
            }
            Feed::Long(by) => {
                bytes.extend(std::iter::repeat_n(7, by));
                None
            }
            Feed::BreaksAt(at) => Some(at),
            _ => None,
        };
        let cancels = match self.feed {
            Feed::CancelsAt(at) => Some((at, self.cancel.clone())),
            _ => None,
        };
        Ok(Box::new(Chunks {
            bytes,
            at: 0,
            breaks_at,
            cancels,
            interrupts_at: match self.feed {
                Feed::InterruptsAt(at) => Some(at),
                _ => None,
            },
        }))
    }
}

fn pattern() -> Vec<u8> {
    (0..SIZE).map(|at| (at % 251) as u8).collect()
}

#[cfg(unix)]
fn local(path: &Path) -> Location {
    use std::os::unix::ffi::OsStrExt;
    Location::local(navo_core::ProviderPath::from_bytes(
        path.as_os_str().as_bytes(),
    ))
    .unwrap()
}

#[cfg(windows)]
fn local(path: &Path) -> Location {
    use std::os::windows::ffi::OsStrExt;
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    Location::local(navo_core::ProviderPath::from_wide(&units)).unwrap()
}

struct Scene {
    dir: tempfile::TempDir,
    plan: OperationPlan,
    sources: Sources,
    cancel: CancelToken,
}

impl Scene {
    fn new(feed: Feed, old: Option<&[u8]>) -> Self {
        Self::with_bytes(feed, old, pattern())
    }

    fn with_bytes(feed: Feed, old: Option<&[u8]>, bytes: Vec<u8>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let target_path = dir.path().join("a.bin");
        if let Some(old) = old {
            std::fs::write(&target_path, old).unwrap();
        }
        let source = Location::new("mem:///far/a.bin").unwrap();
        let target = local(&target_path);
        let conflicts = old
            .map(|_| Conflict {
                source: source.clone(),
                target: target.clone(),
                resolution: Resolution::Overwrite,
            })
            .into_iter()
            .collect();
        let size = bytes.len() as u64;
        let plan = OperationPlan {
            kind: OpKind::Copy,
            steps: vec![Step::CopyFile {
                source,
                target,
                size,
            }],
            totals: Totals {
                files: 1,
                folders: 0,
                bytes: size,
            },
            conflicts,
            risks: Vec::new(),
        };
        let cancel = CancelToken::new();
        let memory = Memory {
            bytes,
            feed,
            cancel: cancel.clone(),
        };
        let sources = Sources::new().with(Arc::new(memory));
        Self {
            dir,
            plan,
            sources,
            cancel,
        }
    }

    fn run(&self, harness: &mut Harness, workers: usize) -> JobOutcome {
        harness.keeps_held = true;
        harness.cancel = self.cancel.clone();
        harness.with(&LocalFs::new(), |ctx| match workers {
            0 => run_job_with_sources(&self.plan, &[], ctx, &self.sources),
            n => run_job_parallel_with_sources((&self.plan, &self.sources), &[], ctx, n),
        })
    }

    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    fn target(&self) -> Vec<u8> {
        std::fs::read(self.dir.path().join("a.bin")).unwrap()
    }
}

fn refused(outcome: &JobOutcome) -> FsErrorKind {
    match outcome {
        JobOutcome::Failed { error, .. } => error.kind,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

mod archive_stream;
mod copy;
mod failures;
#[cfg(unix)]
mod git_stream;
mod immutable;
#[cfg(unix)]
mod retained;
