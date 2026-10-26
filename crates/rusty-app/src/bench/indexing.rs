use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_fs::{IoQueue, LocalFs};
use rusty_index::file::{self, Header};
use rusty_index::host::{Host, HostOptions, HostStatus};
use rusty_index::walker::Limits;

use super::report::{self, Index, Memory};
use crate::metrics;

const POLL: Duration = Duration::from_millis(20);
const PROGRESS_EVERY: Duration = Duration::from_secs(5);

pub(super) const PATH: &str = if cfg!(windows) {
    "walker, the Ctrl+K fallback without the index service (the NTFS MFT reader needs the elevated service and did not run)"
} else {
    "walker, the Ctrl+K disk index on this platform"
};

fn sample() -> Memory {
    let sampled = metrics::Memory::sample();
    Memory {
        private_bytes: sampled.private_bytes,
        resident_bytes: sampled.working_set_bytes,
    }
}

struct Sampler {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Memory>,
}

impl Sampler {
    fn start() -> std::io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let thread = std::thread::Builder::new()
            .name("bench-memory".into())
            .spawn(move || {
                let mut peak = Memory::default();
                while !stopping.load(Ordering::Relaxed) {
                    let now = sample();
                    peak.private_bytes = peak.private_bytes.max(now.private_bytes);
                    peak.resident_bytes = peak.resident_bytes.max(now.resident_bytes);
                    std::thread::sleep(Duration::from_millis(50));
                }
                peak
            })?;
        Ok(Self { stop, thread })
    }

    fn finish(self) -> Memory {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.join().unwrap_or_default()
    }
}

pub(super) fn run(root: &Location, index: &Path) -> Result<(Host, Index), String> {
    let before = sample();
    let sampler =
        Sampler::start().map_err(|error| format!("starting the memory sampler: {error}"))?;
    let started = Instant::now();
    let host = Host::start(HostOptions {
        provider: Arc::new(LocalFs::new()),
        queue: Arc::new(IoQueue::new()),
        root: root.clone(),
        snapshot: index.join("walk.index"),
        limits: Limits {
            max_entries: u32::MAX as usize,
            exclude: Vec::new(),
        },
    })
    .map_err(|error| format!("starting the index: {error}"))?;
    let status = wait_for_scan(&host);
    let elapsed = started.elapsed();
    let peak = sampler.finish();
    if !status.errors.is_empty() {
        return Err(format!("indexing: {}", status.errors.join("; ")));
    }
    let (snapshot_bytes, saved) = save_snapshot(&host, root, &index.join("measured.index"))?;
    let report = Index {
        path: PATH.to_owned(),
        entries: status.entries,
        ms: report::ms(elapsed),
        entries_per_s: status.entries as f64 / elapsed.as_secs_f64().max(f64::EPSILON),
        before,
        peak,
        after: sample(),
        process_peak_working_set_bytes: metrics::Memory::sample().peak_working_set_bytes,
        snapshot_bytes,
        snapshot_save_ms: report::ms(saved),
    };
    Ok((host, report))
}

fn wait_for_scan(host: &Host) -> HostStatus {
    let mut shown = Instant::now();
    loop {
        if let Some(status) = host.try_status() {
            let is_done =
                status.progress.is_none() && (status.entries > 0 || !status.errors.is_empty());
            if is_done {
                return status;
            }
            if shown.elapsed() >= PROGRESS_EVERY {
                eprintln!(
                    "navo bench: indexed {} entries so far",
                    status
                        .progress
                        .map_or(status.entries, |progress| progress.entries)
                );
                shown = Instant::now();
            }
        }
        std::thread::sleep(POLL);
    }
}

fn save_snapshot(host: &Host, root: &Location, path: &Path) -> Result<(u64, Duration), String> {
    let table = host.table();
    let table = table.read().unwrap_or_else(PoisonError::into_inner);
    let header = Header {
        root: root.as_str().to_owned(),
        volume_serial: 0,
        journal_id: 0,
        next_usn: 0,
        built_at: 0,
    };
    let started = Instant::now();
    file::save(path, &header, &table).map_err(|error| format!("saving the index: {error}"))?;
    let saved = started.elapsed();
    let bytes = std::fs::metadata(path)
        .map_err(|error| format!("reading the index size: {error}"))?
        .len();
    Ok((bytes, saved))
}
