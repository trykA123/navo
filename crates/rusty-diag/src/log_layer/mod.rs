use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

use crate::{EventRing, Redactor};

mod json;
mod path_check;
mod rolling;
mod visitor;

pub use path_check::is_redacted_path;

use json::Line;
use rolling::RollingFile;
use visitor::FieldVisitor;

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
        self.count.load(Ordering::Relaxed)
    }

    fn add(&self, misses: u64) {
        if misses > 0 {
            self.count.fetch_add(misses, Ordering::Relaxed);
        }
    }
}

pub struct LogLayer {
    redactor: Arc<Redactor>,
    ring: Arc<EventRing>,
    misses: RedactionMisses,
    file: Option<Mutex<RollingFile>>,
    started: Instant,
}

impl LogLayer {
    pub fn new(
        config: LogConfig,
        redactor: Arc<Redactor>,
        ring: Arc<EventRing>,
    ) -> std::io::Result<Self> {
        let file = config
            .dir
            .map(|dir| {
                RollingFile::open(dir, config.file_stem, config.max_file_bytes, config.files)
            })
            .transpose()?
            .map(Mutex::new);
        Ok(Self {
            redactor,
            ring,
            misses: RedactionMisses::default(),
            file,
            started: Instant::now(),
        })
    }

    #[must_use]
    pub fn misses(&self) -> RedactionMisses {
        self.misses.clone()
    }

    fn render(&self, event: &Event<'_>) -> String {
        let mut visitor = FieldVisitor::new(&self.redactor);
        event.record(&mut visitor);
        self.misses.add(visitor.misses);
        let metadata = event.metadata();
        Line {
            millis: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
            level: metadata.level().as_str(),
            target: metadata.target(),
            msg: &visitor.message,
            fields: &visitor.fields,
        }
        .render()
    }
}

impl<S: Subscriber> Layer<S> for LogLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let line = self.render(event);
        self.ring.push(&line);
        if let Some(file) = &self.file {
            file.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .write_line(&line);
        }
    }
}

#[cfg(test)]
mod tests;
