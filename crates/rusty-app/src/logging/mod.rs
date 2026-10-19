use std::io;
use std::path::Path;
use std::sync::Arc;

use rusty_diag::{CacheRegistry, EventRing, LogConfig, LogLayer, RedactionMisses, Redactor};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

pub const ENV_VAR: &str = "FERRITE_LOG";
const DEFAULT_LEVEL: &str = "info";
const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
const FILES: usize = 5;

pub struct Diagnostics {
    pub redactor: Arc<Redactor>,
    pub ring: Arc<EventRing>,
    pub misses: RedactionMisses,
    pub caches: CacheRegistry,
}

#[derive(Default)]
pub struct Opened {
    pub layer: Option<LogLayer>,
    pub file_error: Option<io::Error>,
}

impl Opened {
    #[must_use]
    pub fn misses(&self) -> RedactionMisses {
        self.layer
            .as_ref()
            .map(LogLayer::misses)
            .unwrap_or_default()
    }
}

pub fn install(data: Option<&Path>) -> Diagnostics {
    let redactor = Arc::new(crate::diag::redactor());
    let ring = Arc::new(EventRing::new(EventRing::DEFAULT_BUDGET));
    if data.is_none() {
        eprintln!("ferrite: no per-user data folder; logging to memory only");
    }
    let opened = open(log_config(data), &redactor, &ring);
    if let Some(error) = &opened.file_error {
        eprintln!("ferrite: cannot open the log folder ({error}); logging to memory only");
    }
    let (filter, problem) = filter(std::env::var(ENV_VAR).ok().as_deref());
    if let Some(problem) = problem {
        eprintln!("ferrite: {problem}");
    }
    let misses = opened.misses();
    let subscriber = Registry::default().with(filter).with(opened.layer);
    if let Err(error) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("ferrite: cannot install logging: {error}");
    }
    let caches = CacheRegistry::default();
    crate::diag::register_events(&caches, &ring);
    Diagnostics {
        redactor,
        ring,
        misses,
        caches,
    }
}

pub fn log_start() {
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        commit = env!("FERRITE_COMMIT"),
        "app started"
    );
}

pub fn log_config(data: Option<&Path>) -> LogConfig {
    LogConfig {
        dir: data.map(|root| root.join("logs")),
        file_stem: "ferrite".to_owned(),
        max_file_bytes: MAX_FILE_BYTES,
        files: FILES,
    }
}

pub fn open(config: LogConfig, redactor: &Arc<Redactor>, ring: &Arc<EventRing>) -> Opened {
    let ring_only = LogConfig {
        dir: None,
        ..config.clone()
    };
    match LogLayer::new(config, Arc::clone(redactor), Arc::clone(ring)) {
        Ok(layer) => Opened {
            layer: Some(layer),
            file_error: None,
        },
        Err(error) => Opened {
            layer: LogLayer::new(ring_only, Arc::clone(redactor), Arc::clone(ring)).ok(),
            file_error: Some(error),
        },
    }
}

pub fn filter(value: Option<&str>) -> (EnvFilter, Option<String>) {
    let Some(directives) = value.map(str::trim).filter(|text| !text.is_empty()) else {
        return (EnvFilter::new(DEFAULT_LEVEL), None);
    };
    match EnvFilter::try_new(directives) {
        Ok(filter) => (filter, None),
        Err(error) => (
            EnvFilter::new(DEFAULT_LEVEL),
            Some(format!(
                "ignoring {ENV_VAR} ({error}); using {DEFAULT_LEVEL}"
            )),
        ),
    }
}

#[cfg(test)]
mod tests;
