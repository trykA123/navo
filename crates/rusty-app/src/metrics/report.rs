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
            version: String::new(),
            commit: String::new(),
            exe_bytes: None,
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
        let _ = (clock, measured);
        Self {
            version: build.version,
            commit: build.commit,
            exe_bytes: build.exe_bytes,
            renderer: String::new(),
            first_frame_ms: 0.0,
            first_usable_ms: 0.0,
            marks: Vec::new(),
            idle_private_bytes: None,
            idle_working_set_bytes: None,
            peak_working_set_bytes: None,
        }
    }

    pub fn to_json(&self) -> String {
        String::new()
    }

    pub fn write(&self, path: &Path) -> io::Result<()> {
        let _ = path;
        Ok(())
    }
}
