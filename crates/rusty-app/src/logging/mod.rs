use std::io;
use std::path::Path;
use std::sync::Arc;

use rusty_diag::{CacheRegistry, EventRing, LogConfig, LogLayer, RedactionMisses, Redactor};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

pub const ENV_VAR: &str = "NAVO_LOG";
pub const LEGACY_ENV_VAR: &str = "FERRITE_LOG";
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
        eprintln!("navo: no per-user data folder; logging to memory only");
    }
    let opened = open(log_config(data), &redactor, &ring);
    if let Some(error) = &opened.file_error {
        eprintln!("navo: cannot open the log folder ({error}); logging to memory only");
    }
    let (filter, warnings) = level(
        std::env::var(ENV_VAR).ok(),
        std::env::var(LEGACY_ENV_VAR).ok(),
    );
    let misses = opened.misses();
    let subscriber = Registry::default().with(filter).with(opened.layer);
    if let Err(error) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("navo: cannot install logging: {error}");
    }
    warn_all(&warnings);
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
        commit = env!("NAVO_COMMIT"),
        "app started"
    );
}

pub fn log_config(data: Option<&Path>) -> LogConfig {
    LogConfig {
        dir: data.map(|root| root.join("logs")),
        file_stem: rusty_core::brand::SLUG.to_owned(),
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

pub fn env_value(
    current: Option<String>,
    legacy: Option<String>,
) -> (Option<String>, &'static str) {
    match (current, legacy) {
        (Some(value), _) => (Some(value), ENV_VAR),
        (None, Some(value)) => (Some(value), LEGACY_ENV_VAR),
        (None, None) => (None, ENV_VAR),
    }
}

pub fn level(current: Option<String>, legacy: Option<String>) -> (EnvFilter, Vec<String>) {
    let (value, var) = env_value(current, legacy);
    let mut warnings = Vec::new();
    if var == LEGACY_ENV_VAR {
        warnings.push(format!("{LEGACY_ENV_VAR} is deprecated; use {ENV_VAR}"));
    }
    let (filter, problem) = filter(value.as_deref(), var);
    warnings.extend(problem);
    (filter, warnings)
}

pub fn warn_all(warnings: &[String]) {
    for warning in warnings {
        tracing::warn!("{warning}");
    }
}

pub fn filter(value: Option<&str>, var: &str) -> (EnvFilter, Option<String>) {
    let Some(directives) = value.map(str::trim).filter(|text| !text.is_empty()) else {
        return (EnvFilter::new(DEFAULT_LEVEL), None);
    };
    match EnvFilter::try_new(directives) {
        Ok(filter) => (filter, None),
        Err(error) => (
            EnvFilter::new(DEFAULT_LEVEL),
            Some(format!("ignoring {var} ({error}); using {DEFAULT_LEVEL}")),
        ),
    }
}

#[cfg(test)]
mod tests;
