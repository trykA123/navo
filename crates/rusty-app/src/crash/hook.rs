use std::backtrace::Backtrace;
use std::fs;
use std::io;
use std::panic::{self, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use rusty_diag::{EventRing, Redactor};
use serde::Serialize;

use crate::logging::Diagnostics;

const KEEP_FILES: usize = 5;
const KEEP_EVENTS: usize = 200;

static SEQUENCE: AtomicU32 = AtomicU32::new(0);

struct Writer {
    logs: PathBuf,
    redactor: Arc<Redactor>,
    ring: Arc<EventRing>,
}

#[derive(Serialize)]
struct Record {
    version: &'static str,
    commit: &'static str,
    thread: String,
    location: Option<String>,
    message: String,
    backtrace: String,
    events: Vec<String>,
}

pub(super) fn install(data: Option<&Path>, diagnostics: &Diagnostics) {
    install_with_flush(data, diagnostics, crate::logging::try_flush);
}

pub(super) fn install_with_flush(
    data: Option<&Path>,
    diagnostics: &Diagnostics,
    flush: impl Fn() + Send + Sync + 'static,
) {
    let Some(data) = data else { return };
    let writer = Writer {
        logs: data.join("logs"),
        redactor: diagnostics.redactor.clone(),
        ring: diagnostics.ring.clone(),
    };
    let serial = Mutex::new(());
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        {
            let _lock = serial.lock().unwrap_or_else(PoisonError::into_inner);
            let _ = writer.write(info);
        }
        flush();
        previous(info);
    }));
}

impl Writer {
    fn write(&self, info: &PanicHookInfo<'_>) -> io::Result<()> {
        let thread = std::thread::current();
        let events = self.ring.snapshot();
        let skip = events.len().saturating_sub(KEEP_EVENTS);
        let record = Record {
            version: env!("CARGO_PKG_VERSION"),
            commit: env!("NAVO_COMMIT"),
            thread: self.redactor.text(thread.name().unwrap_or("unnamed")),
            location: info
                .location()
                .map(|at| format!("{}:{}", short_location(at.file()), at.line())),
            message: self.redactor.text(message(info)),
            backtrace: self
                .redactor
                .backtrace(&Backtrace::force_capture().to_string()),
            events: events.iter().skip(skip).map(ToString::to_string).collect(),
        };
        let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let path = self.logs.join(format!(
            "crash-{stamp}-{}-{:03}.json",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        rusty_store::replace_atomic(&path, &serde_json::to_vec_pretty(&record)?)?;
        let files = files(&self.logs);
        let excess = files.len().saturating_sub(KEEP_FILES);
        for path in files.into_iter().take(excess) {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }
}

fn message<'a>(info: &'a PanicHookInfo<'_>) -> &'a str {
    info.payload()
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
        .unwrap_or("non-string panic payload")
}

pub(super) fn files(logs: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(logs) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_crash)
        })
        .collect();
    files.sort();
    files
}

fn is_crash(name: &str) -> bool {
    let Some(stamp) = name
        .strip_prefix("crash-")
        .and_then(|name| name.strip_suffix(".json"))
    else {
        return false;
    };
    let parts: Vec<_> = stamp.split('-').collect();
    let digits = parts
        .iter()
        .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    matches!(&parts[..], [date, time, _] | [date, time, _, _] if date.len() == 8 && time.len() == 6)
        && digits
}

pub(super) fn short_location(file: &str) -> String {
    let path = file.replace('\\', "/");
    for marker in ["/registry/src/", "/git/checkouts/"] {
        if let Some(at) = path.find(marker) {
            let rest = &path[at + marker.len()..];
            return rest
                .split_once('/')
                .map_or(rest, |(_, tail)| tail)
                .to_owned();
        }
    }
    let absolute = path.starts_with('/') || path.get(1..2) == Some(":");
    if absolute {
        return path.rsplit('/').next().unwrap_or_default().to_owned();
    }
    path
}
