use std::io;
use std::path::Path;
use std::time::Instant;

use serde::Serialize;

use super::clock::ProcessClock;
use super::memory::Memory;

#[derive(Debug, Serialize, PartialEq)]
pub(super) struct Report {
    pub(super) version: String,
    pub(super) commit: String,
    pub(super) exe_bytes: Option<u64>,
    pub(super) renderer: String,
    pub(super) first_frame_ms: f64,
    pub(super) first_usable_ms: f64,
    pub(super) marks: Vec<Mark>,
    pub(super) idle_private_bytes: Option<u64>,
    pub(super) idle_working_set_bytes: Option<u64>,
    pub(super) peak_working_set_bytes: Option<u64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub(super) struct Mark {
    pub(super) name: String,
    pub(super) ms: f64,
}

pub(super) struct Build {
    pub(super) version: String,
    pub(super) commit: String,
    pub(super) exe_bytes: Option<u64>,
}

impl Build {
    pub(super) fn current() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            commit: env!("NAVO_COMMIT").to_owned(),
            exe_bytes: std::env::current_exe()
                .and_then(std::fs::metadata)
                .ok()
                .map(|meta| meta.len()),
        }
    }
}

pub(super) struct Measured {
    pub(super) renderer: Option<String>,
    pub(super) first_frame: Instant,
    pub(super) first_usable: Instant,
    pub(super) marks: Vec<(&'static str, Instant)>,
    pub(super) memory: Memory,
}

impl Report {
    pub(super) fn new(build: Build, clock: &ProcessClock, measured: Measured) -> Self {
        let memory = measured.memory;
        Self {
            version: build.version,
            commit: build.commit,
            exe_bytes: build.exe_bytes,
            renderer: measured.renderer.unwrap_or_else(|| "unknown".to_owned()),
            first_frame_ms: clock.ms(measured.first_frame),
            first_usable_ms: clock.ms(measured.first_usable),
            marks: measured
                .marks
                .into_iter()
                .map(|(name, at)| Mark {
                    name: name.to_owned(),
                    ms: clock.ms(at),
                })
                .collect(),
            idle_private_bytes: memory.private_bytes,
            idle_working_set_bytes: memory.working_set_bytes,
            peak_working_set_bytes: memory.peak_working_set_bytes,
        }
    }

    pub(super) fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub(super) fn write(&self, path: &Path) -> io::Result<()> {
        std::fs::write(path, self.to_json() + "\n")
    }
}
