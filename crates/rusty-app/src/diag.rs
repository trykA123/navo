use std::io;
use std::num::NonZero;
use std::path::Path;
use std::time::Instant;

use rusty_diag::{
    BuildInfo, CacheRegistry, Identity, MemoryProbe, Redactor, ReportInput, ReportSummary, Salt,
    SystemInfo, TaskCounters, memory_report, write_report,
};

use crate::logging::Diagnostics;

const NO_WINDOW: &str = "unknown (no window)";

pub fn write(
    path: &Path,
    started: Instant,
    diagnostics: &Diagnostics,
) -> io::Result<ReportSummary> {
    let mut input = report_input();
    input.events = diagnostics.ring.snapshot();
    input.redaction_misses = diagnostics.misses.count();
    input.memory = Some(memory_report(
        probe().as_ref(),
        &CacheRegistry::default(),
        &TaskCounters::default(),
    ));
    input.startup_ms = vec![(
        "report".to_owned(),
        started.elapsed().as_secs_f64() * 1000.0,
    )];
    write_report(path, &input, &diagnostics.redactor)
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
            commit: env!("FERRITE_COMMIT").to_owned(),
        },
        system: SystemInfo {
            os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
            cpu_count: std::thread::available_parallelism().map_or(0, NonZero::get),
            ram_bytes: total_ram(),
            gpu: NO_WINDOW.to_owned(),
            scale: 0.0,
        },
        ..ReportInput::default()
    }
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
