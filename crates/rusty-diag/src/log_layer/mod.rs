use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

use crate::{EventRing, Redactor};

#[derive(Debug, Clone)]
pub struct LogConfig {
    pub dir: Option<PathBuf>,
    pub file_stem: String,
    pub max_file_bytes: u64,
    pub files: usize,
}

#[derive(Clone, Default)]
pub struct RedactionMisses {
    count: Arc<AtomicU64>,
}

impl RedactionMisses {
    #[must_use]
    pub fn count(&self) -> u64 {
        todo!()
    }
}

pub struct LogLayer {
    redactor: Arc<Redactor>,
    ring: Arc<EventRing>,
    misses: RedactionMisses,
}

impl LogLayer {
    pub fn new(
        _config: LogConfig,
        _redactor: Arc<Redactor>,
        _ring: Arc<EventRing>,
    ) -> std::io::Result<Self> {
        todo!()
    }

    #[must_use]
    pub fn misses(&self) -> RedactionMisses {
        todo!()
    }
}

impl<S: Subscriber> Layer<S> for LogLayer {
    fn on_event(&self, _event: &Event<'_>, _ctx: Context<'_, S>) {
        todo!()
    }
}

#[must_use]
pub fn is_redacted_path(_text: &str) -> bool {
    todo!()
}

#[cfg(test)]
mod tests;
