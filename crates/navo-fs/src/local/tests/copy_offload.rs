//! Copy offload probe (R12a): copies on an SMB share and on a Dev Drive and measures how many
//! bytes this test process read and wrote, to see whether the server or the volume did the copy.
//! It only measures; offload is never asserted. Run by `scripts/offload-probe.ps1` on Windows.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;

use navo_core::{ConflictPolicy, OpKind, OperationPlan, PlanRequest, plan};

use super::ops::{Log, child};
use super::*;
use crate::{JobContext, JobOutcome, ProviderOps, run_job, scan};

const MIB: u64 = 1024 * 1024;
/// Below `AutoTuning::unbuffered_from` (256 MiB), so an unowned copy stays buffered.
const SMALL: u64 = 64 * MIB;
/// Above `AutoTuning::unbuffered_from`, so an unowned copy sets `COPY_FILE_NO_BUFFERING`.
const LARGE: u64 = 300 * MIB;
const CHUNK: usize = 1024 * 1024;

#[derive(Clone, Copy)]
enum Target {
    Smb,
    DevDrive,
}

impl Target {
    fn name(self) -> &'static str {
        match self {
            Self::Smb => "smb",
            Self::DevDrive => "devdrive",
        }
    }

    /// The folder this target's env var names, or `None` when the var is unset or empty.
    fn root(self) -> Option<PathBuf> {
        let var = match self {
            Self::Smb => "NAVO_TEST_SMB_DIR",
            Self::DevDrive => "NAVO_TEST_DEVDRIVE_DIR",
        };
        std::env::var_os(var)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }
}

#[derive(Clone, Copy)]
enum Case {
    SmallUnowned,
    LargeUnowned,
    LargeJob,
}

impl Case {
    fn name(self) -> &'static str {
        match self {
            Self::SmallUnowned => "small_unowned",
            Self::LargeUnowned => "large_unowned",
            Self::LargeJob => "large_job",
        }
    }

    fn size(self) -> u64 {
        match self {
            Self::SmallUnowned => SMALL,
            Self::LargeUnowned | Self::LargeJob => LARGE,
        }
    }
}

/// Bytes this process has read and written through I/O calls since it started.
struct IoCount {
    read: u64,
    write: u64,
}

/// Reads the process I/O counters; panics (fails the probe) when the call fails.
#[allow(
    unsafe_code,
    reason = "the probe reads the process I/O counters through the Win32 API"
)]
fn io_count() -> IoCount {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetProcessIoCounters, IO_COUNTERS,
    };

    let mut counters = IO_COUNTERS {
        ReadOperationCount: 0,
        WriteOperationCount: 0,
        OtherOperationCount: 0,
        ReadTransferCount: 0,
        WriteTransferCount: 0,
        OtherTransferCount: 0,
    };
    // SAFETY: the pseudo handle needs no closing and `counters` outlives the call.
    let ok = unsafe { GetProcessIoCounters(GetCurrentProcess(), &raw mut counters) };
    assert_ne!(
        ok,
        0,
        "GetProcessIoCounters: {}",
        std::io::Error::last_os_error()
    );
    IoCount {
        read: counters.ReadTransferCount,
        write: counters.WriteTransferCount,
    }
}

/// Deterministic xorshift64 byte stream, so the content check needs no second copy of the source.
struct Xorshift(u64);

impl Xorshift {
    fn fill(&mut self, buffer: &mut [u8]) {
        for word in buffer.chunks_mut(8) {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            word.copy_from_slice(&self.0.to_le_bytes()[..word.len()]);
        }
    }
}

const SEED: u64 = 0x9e37_79b9_7f4a_7c15;

fn write_source(path: &Path, size: u64) {
    let mut stream = Xorshift(SEED);
    let mut buffer = vec![0_u8; CHUNK];
    let mut out = BufWriter::new(File::create(path).unwrap());
    let mut left = size;
    while left > 0 {
        let take = usize::try_from(left.min(CHUNK as u64)).unwrap();
        stream.fill(&mut buffer[..take]);
        out.write_all(&buffer[..take]).unwrap();
        left -= take as u64;
    }
    out.into_inner().unwrap().sync_all().unwrap();
}

/// Checks that `path` holds exactly the xorshift stream of `size` bytes; names the first bad MiB.
fn assert_same_content(path: &Path, size: u64) {
    let mut stream = Xorshift(SEED);
    let mut expected = vec![0_u8; CHUNK];
    let mut actual = vec![0_u8; CHUNK];
    let mut file = File::open(path).unwrap();
    assert_eq!(file.metadata().unwrap().len(), size, "{}", path.display());
    let mut offset = 0;
    while offset < size {
        let take = usize::try_from((size - offset).min(CHUNK as u64)).unwrap();
        stream.fill(&mut expected[..take]);
        file.read_exact(&mut actual[..take]).unwrap();
        assert!(
            expected[..take] == actual[..take],
            "content differs in the MiB at offset {offset} of {}",
            path.display()
        );
        offset += take as u64;
    }
}

fn job_plan(ops: &LocalFs, root: &Location) -> OperationPlan {
    let items = scan(ops, &[child(root, "source.bin")], &CancelToken::new()).unwrap();
    plan(&PlanRequest {
        kind: OpKind::Copy,
        items,
        target: Some(child(root, "dst")),
        existing: Vec::new(),
        policy: ConflictPolicy::Skip,
        same_volume: true,
        protected: Vec::new(),
    })
}

/// Copies `source.bin` the way `case` says; returns the copy's file name and every progress value.
fn run_case(case: Case, dir: &Path, root: &Location) -> (PathBuf, Vec<u64>) {
    let ops = LocalFs::new();
    let cancel = CancelToken::new();
    let mut seen = Vec::new();
    if let Case::LargeJob = case {
        std::fs::create_dir(dir.join("dst")).unwrap();
        let plan = job_plan(&ops, root);
        let mut log = Log::default();
        let outcome = run_job(
            &plan,
            &[],
            &mut JobContext {
                ops: &ops,
                log: &mut log,
                cancel: &cancel,
                progress: &mut |progress| seen.push(progress.bytes_done),
            },
        );
        assert_eq!(outcome, JobOutcome::Done);
        return (dir.join("dst").join("source.bin"), seen);
    }
    ops.copy_file(
        &child(root, "source.bin"),
        &child(root, "target.bin"),
        &mut |bytes| seen.push(bytes),
        &cancel,
    )
    .unwrap();
    (dir.join("target.bin"), seen)
}

/// Appends one JSON line for the case to `NAVO_TEST_OFFLOAD_OUT`, when that var is set.
fn record(target: Target, case: Case, used: &IoCount, ms: u128) {
    let Some(out) = std::env::var_os("NAVO_TEST_OFFLOAD_OUT").filter(|value| !value.is_empty())
    else {
        return;
    };
    let size = case.size();
    let line = format!(
        "{{\"target\":\"{}\",\"case\":\"{}\",\"size\":{size},\"read_bytes\":{},\"write_bytes\":{},\"ms\":{ms},\"offloaded\":{}}}\n",
        target.name(),
        case.name(),
        used.read,
        used.write,
        used.read < size / 100,
    );
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(out)
        .unwrap();
    file.write_all(line.as_bytes()).unwrap();
}

/// Runs one case on one target: skipped when the target's env var is unset.
fn probe(target: Target, case: Case) {
    let Some(base) = target.root() else {
        return;
    };
    let dir = tempfile::tempdir_in(&base).unwrap();
    let root = location_of(dir.path());
    write_source(&dir.path().join("source.bin"), case.size());
    let before = io_count();
    let started = Instant::now();
    let (copy, seen) = run_case(case, dir.path(), &root);
    let ms = started.elapsed().as_millis();
    let after = io_count();
    let used = IoCount {
        read: after.read - before.read,
        write: after.write - before.write,
    };
    assert_same_content(&copy, case.size());
    assert_eq!(seen.last(), Some(&case.size()));
    record(target, case, &used, ms);
}

#[test]
#[ignore = "requires NAVO_TEST_SMB_DIR or NAVO_TEST_DEVDRIVE_DIR"]
fn smb_small_unowned() {
    probe(Target::Smb, Case::SmallUnowned);
}

#[test]
#[ignore = "requires NAVO_TEST_SMB_DIR or NAVO_TEST_DEVDRIVE_DIR"]
fn smb_large_unowned() {
    probe(Target::Smb, Case::LargeUnowned);
}

#[test]
#[ignore = "requires NAVO_TEST_SMB_DIR or NAVO_TEST_DEVDRIVE_DIR"]
fn smb_large_job() {
    probe(Target::Smb, Case::LargeJob);
}

#[test]
#[ignore = "requires NAVO_TEST_SMB_DIR or NAVO_TEST_DEVDRIVE_DIR"]
fn devdrive_small_unowned() {
    probe(Target::DevDrive, Case::SmallUnowned);
}

#[test]
#[ignore = "requires NAVO_TEST_SMB_DIR or NAVO_TEST_DEVDRIVE_DIR"]
fn devdrive_large_unowned() {
    probe(Target::DevDrive, Case::LargeUnowned);
}

#[test]
#[ignore = "requires NAVO_TEST_SMB_DIR or NAVO_TEST_DEVDRIVE_DIR"]
fn devdrive_large_job() {
    probe(Target::DevDrive, Case::LargeJob);
}
