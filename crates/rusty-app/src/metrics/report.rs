use std::io;
use std::path::Path;
use std::time::Instant;

use serde::Serialize;

use super::clock::ProcessClock;
use super::memory::Memory;

#[derive(Debug, Serialize, PartialEq)]
pub struct Report {
    pub version: String,
    pub commit: String,
    pub exe_bytes: Option<u64>,
    pub renderer: String,
    pub first_frame_ms: f64,
    pub first_usable_ms: f64,
    pub marks: Vec<Mark>,
    pub idle_private_bytes: Option<u64>,
    pub idle_working_set_bytes: Option<u64>,
    pub peak_working_set_bytes: Option<u64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Mark {
    pub name: String,
    pub ms: f64,
}

pub struct Build {
    pub version: String,
    pub commit: String,
    pub exe_bytes: Option<u64>,
}

impl Build {
    pub fn current() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_owned(),
            commit: env!("FERRITE_COMMIT").to_owned(),
            exe_bytes: std::env::current_exe()
                .and_then(std::fs::metadata)
                .ok()
                .map(|meta| meta.len()),
        }
    }
}

pub struct Measured {
    pub renderer: Option<String>,
    pub first_frame: Instant,
    pub first_usable: Instant,
    pub marks: Vec<(&'static str, Instant)>,
    pub memory: Memory,
}

impl Report {
    pub fn new(build: Build, clock: &ProcessClock, measured: Measured) -> Self {
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

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn write(&self, path: &Path) -> io::Result<()> {
        std::fs::write(path, self.to_json() + "\n")
    }
}
