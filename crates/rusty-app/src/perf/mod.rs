use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use rusty_diag::ReportAttachments;
use rusty_diag::perf::{
    PerfDocument, ProcessSample, Recording, SystemFacts, ThreadCpu, perf_json, threads_text,
};
use rusty_ui::perf::PerfService;

use crate::logging::Diagnostics;

mod process;
mod system;
mod threads;

#[derive(Clone, Copy)]
struct CpuMark {
    at: Instant,
    used: Duration,
}

pub(crate) struct PerfReports {
    started: Instant,
    diagnostics: Diagnostics,
    folder: Option<PathBuf>,
    data: Option<PathBuf>,
    last_cpu: Mutex<Option<CpuMark>>,
    threads: Mutex<ThreadCpu>,
}

pub(crate) fn install(started: Instant, diagnostics: &Diagnostics, data: Option<&Path>) {
    rusty_ui::perf::install(Arc::new(PerfReports {
        started,
        diagnostics: diagnostics.clone(),
        folder: crate::crash::desktop_dir(),
        data: data.map(Path::to_path_buf),
        last_cpu: Mutex::new(None),
        threads: Mutex::new(ThreadCpu::default()),
    }));
}

impl PerfReports {
    fn cpu_percent(&self, now: Instant) -> Option<f32> {
        let used = process::cpu_time()?;
        let mut last = self.last_cpu.lock().unwrap_or_else(PoisonError::into_inner);
        let previous = last.replace(CpuMark { at: now, used })?;
        let wall = now.saturating_duration_since(previous.at).as_secs_f32();
        let busy = used.saturating_sub(previous.used).as_secs_f32();
        (wall > 0.0).then(|| busy / wall * 100.0)
    }

    fn sample_threads(&self) {
        let sampled = threads::sample();
        let now = Instant::now();
        let mut tracker = self.threads.lock().unwrap_or_else(PoisonError::into_inner);
        tracker.record(now, &sampled);
    }
}

impl PerfService for PerfReports {
    fn start(&self) {
        *self.last_cpu.lock().unwrap_or_else(PoisonError::into_inner) = None;
        *self.threads.lock().unwrap_or_else(PoisonError::into_inner) = ThreadCpu::default();
    }

    fn sample(&self) -> ProcessSample {
        let memory = crate::diag::probe().sample().ok();
        self.sample_threads();
        ProcessSample {
            at_ms: 0.0,
            cpu_percent: self.cpu_percent(Instant::now()),
            working_set_bytes: memory.map(|memory| memory.resident_bytes),
            private_bytes: memory.map(|memory| memory.private_bytes),
            threads: process::threads(),
        }
    }

    fn system(&self) -> SystemFacts {
        system::facts()
    }

    fn save(
        &self,
        recording: &Recording,
        ended: Instant,
        system: &SystemFacts,
    ) -> Result<PathBuf, String> {
        let folder = self
            .folder
            .as_deref()
            .ok_or("no Desktop or home folder was found")?;
        let redactor = &self.diagnostics.redactor;
        let report = self
            .threads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .report();
        let document =
            PerfDocument::new(recording, ended, system, redactor).with_threads(&report, redactor);
        let json = perf_json(&document).map_err(|error| error.to_string())?;
        let text = threads_text(&report, redactor);
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
        let disk_use = crate::diag::disk_use(self.data.as_deref(), &self.diagnostics.redactor);
        save_zip(folder, &stamp, |path| {
            crate::diag::write_with_extras(
                path,
                self.started,
                &self.diagnostics,
                ReportAttachments {
                    disk_use: disk_use.clone(),
                    extras: &[("perf.json", &json), ("perf-threads.txt", text.as_bytes())],
                    ..ReportAttachments::default()
                },
            )
        })
        .map_err(|error| error.to_string())
    }
}

fn save_zip(
    folder: &Path,
    stamp: &str,
    write: impl Fn(&Path) -> io::Result<()>,
) -> io::Result<PathBuf> {
    let mut last = io::Error::from(io::ErrorKind::AlreadyExists);
    for attempt in 1..=9 {
        let suffix = if attempt == 1 {
            String::new()
        } else {
            format!("-{attempt}")
        };
        let path = folder.join(format!(
            "{}-performance-{stamp}{suffix}.zip",
            rusty_core::brand::SLUG
        ));
        match write(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => last = error,
            Err(error) => return Err(error),
        }
    }
    Err(last)
}

#[cfg(test)]
mod tests;
