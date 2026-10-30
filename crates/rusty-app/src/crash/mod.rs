use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
use std::{fs, io};

use crate::logging::Diagnostics;

mod downloads;
mod hook;
pub(crate) mod store;
#[cfg(test)]
pub(crate) mod test_damage;
mod unclean;

#[cfg(test)]
pub(crate) use unclean::sentinel_path;
pub(crate) use unclean::{open_db, record_failed_check};

static RUN_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub(crate) struct Pending {
    pub(crate) files: Vec<PathBuf>,
    marker: PathBuf,
    run: Vec<u8>,
}

pub(crate) struct Running(Pending);

impl Running {
    pub(crate) fn start(data: &Path) -> io::Result<Self> {
        fs::create_dir_all(data)?;
        let marker = data.join("running");
        let run = format!(
            "{}-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default(),
            RUN_ID.fetch_add(1, Ordering::Relaxed)
        )
        .into_bytes();
        rusty_store::replace_atomic(&marker, &run)?;
        Ok(Self(Pending {
            files: Vec::new(),
            marker,
            run,
        }))
    }

    pub(crate) fn finish(&self) -> io::Result<()> {
        clear(&self.0)
    }
}

pub(crate) fn install(data: Option<&Path>, diagnostics: &Diagnostics) {
    hook::install(data, diagnostics);
}

pub(crate) fn pending(data: &Path) -> Option<Pending> {
    let marker = data.join("running");
    let run = fs::read(&marker).ok()?;
    Some(Pending {
        files: hook::files(&data.join("logs")),
        marker,
        run,
    })
}

pub(crate) fn clear(pending: &Pending) -> io::Result<()> {
    match fs::read(&pending.marker) {
        Ok(run) if run == pending.run => fs::remove_file(&pending.marker),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(crate) fn save_report_files(
    files: &[PathBuf],
    started: Instant,
    diagnostics: &Diagnostics,
    downloads: &Path,
) -> io::Result<PathBuf> {
    let stamp = chrono::Utc::now().format("%Y%m%d-%H%M");
    let mut last = io::Error::from(io::ErrorKind::AlreadyExists);
    for attempt in 1..=9 {
        let suffix = if attempt == 1 {
            String::new()
        } else {
            format!("-{attempt}")
        };
        let path = downloads.join(format!("navo-report-{stamp}{suffix}.zip"));
        match crate::diag::write_with_crashes(&path, started, diagnostics, files) {
            Ok(_) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => last = error,
            Err(error) => return Err(error),
        }
    }
    Err(last)
}

pub(crate) fn downloads_dir() -> Option<PathBuf> {
    downloads::resolve()
}

#[cfg(feature = "tester")]
pub(crate) fn desktop_dir() -> Option<PathBuf> {
    downloads::resolve_desktop()
}

#[cfg(test)]
mod tests;
