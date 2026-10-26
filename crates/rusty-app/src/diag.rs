use std::io;
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use rusty_cache::{DiskListings, DiskStats, ListingCache};
use rusty_diag::{
    BuildInfo, CacheRegistry, CacheSize, DpiAwareness, EventRing, Identity, MemoryProbe, Redactor,
    ReportInput, ReportSummary, Salt, SystemInfo, TaskCounters, memory_report,
    write_report_with_crashes,
};

use crate::logging::Diagnostics;

const NO_WINDOW: &str = "unknown (no window)";

pub fn write(
    path: &Path,
    started: Instant,
    diagnostics: &Diagnostics,
) -> io::Result<ReportSummary> {
    write_with_crashes(path, started, diagnostics, &[])
}

pub fn write_on_demand(
    path: &Path,
    started: Instant,
    diagnostics: &Diagnostics,
    data: Option<&Path>,
) -> io::Result<ReportSummary> {
    if let Some(stats) = data.and_then(|data| DiskListings::stored(&data.join("cache.db"))) {
        listings_on_disk(&diagnostics.caches, stats.budget, move || stats);
    }
    write(path, started, diagnostics)
}

pub(crate) fn write_with_crashes(
    path: &Path,
    started: Instant,
    diagnostics: &Diagnostics,
    crashes: &[PathBuf],
) -> io::Result<ReportSummary> {
    let mut input = report_input();
    input.events = diagnostics.ring.snapshot();
    input.redaction_misses = diagnostics.misses.count();
    input.memory = Some(memory_report(
        probe().as_ref(),
        &diagnostics.caches,
        &TaskCounters::default(),
    ));
    input.startup_ms = vec![(
        "report".to_owned(),
        started.elapsed().as_secs_f64() * 1000.0,
    )];
    write_report_with_crashes(path, &input, &diagnostics.redactor, crashes)
}

pub(crate) fn register_listings(
    caches: &CacheRegistry,
    cache: &ListingCache,
    disk: Option<&DiskListings>,
) {
    let memory = cache.clone();
    let budget = memory.stats().budget;
    caches.register("listings in memory", wide(budget), move || {
        let stats = memory.stats();
        CacheSize {
            bytes: wide(stats.bytes),
            pinned_bytes: wide(stats.pinned_bytes),
            entries: wide(stats.entries),
        }
    });
    if let Some(disk) = disk.cloned() {
        listings_on_disk(caches, disk.stats().budget, move || disk.stats());
    }
}

fn listings_on_disk(
    caches: &CacheRegistry,
    budget: u64,
    measure: impl Fn() -> DiskStats + Send + Sync + 'static,
) {
    caches.register("listings in cache.db", budget, move || {
        let stats = measure();
        CacheSize {
            bytes: stats.bytes,
            pinned_bytes: 0,
            entries: stats.entries,
        }
    });
}

pub(crate) fn register_events(caches: &CacheRegistry, ring: &Arc<EventRing>) {
    let ring = Arc::clone(ring);
    caches.register("diagnostic events", wide(ring.budget()), move || {
        let stats = ring.stats();
        CacheSize {
            bytes: wide(stats.bytes),
            pinned_bytes: 0,
            entries: wide(stats.events),
        }
    });
}

fn wide(count: usize) -> u64 {
    u64::try_from(count).unwrap_or(u64::MAX)
}

pub fn redactor() -> Redactor {
    let mut identity = identity(|name| std::env::var(name).ok());
    if identity.computer.is_none() {
        identity.computer = host_name();
    }
    Redactor::new(Salt::random(), identity)
}

pub fn identity(var: impl Fn(&str) -> Option<String>) -> Identity {
    let first = |names: &[&str]| {
        names
            .iter()
            .find_map(|name| var(name).filter(|value| !value.is_empty()))
    };
    Identity {
        user: first(&["USERNAME", "USER"]),
        computer: first(&["COMPUTERNAME", "HOSTNAME"]),
        domain: first(&["USERDOMAIN"]),
        profile: first(&["USERPROFILE", "HOME"]),
    }
}

pub fn report_input() -> ReportInput {
    ReportInput {
        build: BuildInfo {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            commit: env!("NAVO_COMMIT").to_owned(),
        },
        system: SystemInfo {
            os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            cpu_count: std::thread::available_parallelism().map_or(0, NonZero::get),
            ram_bytes: total_ram(),
            gpu: NO_WINDOW.to_owned(),
            scale: 0.0,
            dpi_awareness: dpi_awareness(),
        },
        ..ReportInput::default()
    }
}

#[cfg(windows)]
fn dpi_awareness() -> DpiAwareness {
    use windows_sys::Win32::UI::HiDpi::{
        AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, DPI_AWARENESS_CONTEXT_SYSTEM_AWARE,
        DPI_AWARENESS_CONTEXT_UNAWARE, DPI_AWARENESS_CONTEXT_UNAWARE_GDISCALED,
        GetThreadDpiAwarenessContext,
    };

    // SAFETY: These APIs accept the current thread context and predefined context constants.
    unsafe {
        let context = GetThreadDpiAwarenessContext();
        [
            (
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                DpiAwareness::PerMonitorV2,
            ),
            (
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
                DpiAwareness::PerMonitor,
            ),
            (DPI_AWARENESS_CONTEXT_SYSTEM_AWARE, DpiAwareness::System),
            (DPI_AWARENESS_CONTEXT_UNAWARE, DpiAwareness::Unaware),
            (
                DPI_AWARENESS_CONTEXT_UNAWARE_GDISCALED,
                DpiAwareness::Unaware,
            ),
        ]
        .into_iter()
        .find(|(known, _)| AreDpiAwarenessContextsEqual(context, *known) != 0)
        .map_or(DpiAwareness::Unknown, |(_, awareness)| awareness)
    }
}

#[cfg(not(windows))]
fn dpi_awareness() -> DpiAwareness {
    DpiAwareness::Unknown
}

#[cfg(any(not(windows), test))]
pub fn parse_mem_total(meminfo: &str) -> Option<u64> {
    let line = meminfo.lines().find(|line| line.starts_with("MemTotal:"))?;
    let kilobytes = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    kilobytes.checked_mul(1024)
}

#[cfg(target_os = "linux")]
pub fn probe() -> Box<dyn MemoryProbe> {
    Box::new(rusty_diag::ProcStatusProbe)
}

#[cfg(windows)]
pub fn probe() -> Box<dyn MemoryProbe> {
    Box::new(crate::windows_memory::WindowsProbe)
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn probe() -> Box<dyn MemoryProbe> {
    Box::new(rusty_diag::FakeProbe(
        Err(io::ErrorKind::Unsupported.into()),
    ))
}

#[cfg(windows)]
fn total_ram() -> u64 {
    crate::windows_memory::total_ram()
}

#[cfg(not(windows))]
fn total_ram() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| parse_mem_total(&text))
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn host_name() -> Option<String> {
    let name = std::fs::read_to_string("/proc/sys/kernel/hostname").ok()?;
    Some(name.trim().to_owned()).filter(|name| !name.is_empty())
}

#[cfg(not(target_os = "linux"))]
fn host_name() -> Option<String> {
    None
}

#[cfg(test)]
mod tests;
