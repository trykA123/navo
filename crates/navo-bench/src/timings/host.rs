use std::time::Duration;

use serde::Serialize;

use super::error::TimingError;
use crate::probe::{CloseOutcome, Key};

/// Which folder a timing is taken on.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Subject {
    /// The folder the owner chose; Navo only reads it.
    Owner,
    /// The 100,000 empty files the tool made inside the run root.
    Generated,
}

/// A window start of Navo on `subject`. `run` numbers the profile and output files.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Launch {
    pub subject: Subject,
    pub run: u32,
    pub metrics: bool,
    pub trace: bool,
}

/// Which output file of a run to read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Capture {
    Metrics,
    Trace,
}

/// A running Navo window. Dropping it ends the process tree.
pub trait Window {
    fn wait_for_window(&mut self, timeout: Duration) -> Result<Duration, TimingError>;
    /// Posts each key to Navo's own window only.
    fn post_keys(&mut self, keys: &[Key]) -> Result<(), TimingError>;
    fn close(self: Box<Self>, deadline: Duration) -> Result<CloseOutcome, TimingError>;
}

/// Everything the folder timings need from the machine; tests inject a fake.
pub trait Host {
    /// Creates the working folders, only through the guard.
    fn prepare(&mut self) -> Result<(), TimingError>;
    /// Creates the generated folder with `files` empty files, only through the guard.
    fn generate(&mut self, files: u32) -> Result<(), TimingError>;
    /// Runs `navo bench --folder` to its end and returns the JSON it wrote.
    fn run_bench(&mut self, subject: Subject, run: u32) -> Result<String, TimingError>;
    fn launch(&mut self, launch: Launch) -> Result<Box<dyn Window>, TimingError>;
    /// The text of a run's output file, once it exists.
    fn read(&mut self, capture: Capture, run: u32) -> Option<String>;
    fn pause(&mut self, time: Duration);
    /// Time since the host was made.
    fn elapsed(&self) -> Duration;
}
