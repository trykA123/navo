use std::fmt::Debug;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use navo_diag::{
    CacheRegistry, EventRing, Identity, LogConfig, LogFlush, LogLayer, Redactor, Salt,
};
use tracing::field::{Field, Visit};
use tracing_subscriber::Registry;
use tracing_subscriber::layer::{Context, Layer, SubscriberExt};

use super::check::{Hay, files_in, hay, zip_entries};
use crate::logging::Diagnostics;

#[derive(Default, Clone)]
struct Lines(Arc<Mutex<Vec<String>>>);

struct Fields(String);

impl Visit for Fields {
    fn record_debug(&mut self, field: &Field, value: &dyn Debug) {
        self.0.push_str(&format!(" {}={value:?}", field.name()));
    }
}

impl<S: tracing::Subscriber> Layer<S> for Lines {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let mut fields = Fields(event.metadata().target().to_owned());
        event.record(&mut fields);
        self.0.lock().unwrap().push(fields.0);
    }
}

/// The global subscriber of one test process: an unredacted line capture and navo-diag's own
/// log layer (log files and event ring).
pub(super) struct Spy {
    lines: Lines,
    flush: LogFlush,
    pub(super) diagnostics: Diagnostics,
    pub(super) app: PathBuf,
}

impl Spy {
    /// Each test runs in its own process (nextest), so the subscriber is global.
    pub(super) fn install(app: &Path) -> Self {
        let redactor = Arc::new(Redactor::new(
            Salt::from_bytes([7; 16]),
            Identity::default(),
        ));
        let ring = Arc::new(EventRing::new(EventRing::DEFAULT_BUDGET));
        let logs = app.join("logs");
        fs::create_dir_all(&logs).unwrap();
        let config = LogConfig {
            dir: Some(logs),
            file_stem: "navo".to_owned(),
            max_file_bytes: 10 * 1024 * 1024,
            files: 3,
        };
        let layer = LogLayer::new(config, redactor.clone(), ring.clone()).unwrap();
        let (misses, flush) = (layer.misses(), layer.flush_handle());
        let lines = Lines::default();
        let subscriber = Registry::default().with(lines.clone()).with(layer);
        tracing::subscriber::set_global_default(subscriber)
            .expect("one subscriber per process: run the privacy tests with nextest");
        Self {
            lines,
            flush,
            diagnostics: Diagnostics {
                redactor,
                ring,
                misses,
                caches: CacheRegistry::default(),
            },
            app: app.to_owned(),
        }
    }

    pub(super) fn raw(&self) -> Vec<Hay> {
        let lines = self.lines.0.lock().unwrap().join("\n");
        vec![hay("raw log lines", lines)]
    }

    pub(super) fn ring(&self) -> Vec<Hay> {
        let events = self.diagnostics.ring.snapshot();
        let joined: Vec<&str> = events.iter().map(|event| &**event).collect();
        vec![hay("event ring", joined.join("\n"))]
    }

    /// The log files and crash records in the logs folder.
    pub(super) fn files(&self) -> Vec<Hay> {
        self.flush.flush();
        files_in(&self.app.join("logs"))
    }

    /// Installs navo's panic hook and panics once, so the folder holds a crash record.
    pub(super) fn crash(&self, message: &str) {
        crate::crash::install(Some(&self.app), &self.diagnostics);
        let result = panic::catch_unwind(AssertUnwindSafe(|| panic!("{message}")));
        assert!(result.is_err());
        assert!(!self.crash_records().is_empty(), "the hook wrote no record");
    }

    pub(super) fn crash_records(&self) -> Vec<Hay> {
        self.files()
            .into_iter()
            .filter(|(name, _)| name.starts_with("crash-"))
            .collect()
    }

    /// Writes the diagnostic report with the crash records attached and reads it back.
    pub(super) fn report(&self) -> Vec<Hay> {
        let crashes: Vec<PathBuf> = self
            .crash_records()
            .iter()
            .map(|(name, _)| self.app.join("logs").join(name))
            .collect();
        let zip = self.app.join("report.zip");
        let started = Instant::now();
        crate::diag::write_with_crashes(&zip, started, &self.diagnostics, &crashes).unwrap();
        zip_entries(&zip)
    }
}
