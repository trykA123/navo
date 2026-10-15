use std::path::Path;
use std::sync::Arc;

use serde_json::{Map, Value};
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;

use super::{LogConfig, LogLayer, RedactionMisses, is_redacted_path};
use crate::{DriveKind, EventRing, Identity, PathKind, RedactedPath, Redactor, Salt};

mod canary;
mod fields;
mod format;
mod paths;
mod rolling;

const TARGET: &str = "ferrite::test";

fn identity() -> Identity {
    Identity {
        user: Some("canary-user-7731".to_owned()),
        computer: Some("CANARY-PC-7731".to_owned()),
        domain: Some("CANARYDOM".to_owned()),
        profile: Some(r"C:\Users\canary-user-7731".to_owned()),
    }
}

fn redactor() -> Arc<Redactor> {
    Arc::new(Redactor::new(Salt::from_bytes([7; 16]), identity()))
}

fn redacted(r: &Redactor, path: &str) -> RedactedPath {
    r.path(path, PathKind::File, DriveKind::Local)
}

fn config(dir: Option<&Path>, max_file_bytes: u64) -> LogConfig {
    LogConfig {
        dir: dir.map(Path::to_path_buf),
        file_stem: "ferrite".to_owned(),
        max_file_bytes,
        files: 3,
    }
}

fn ring_only() -> LogConfig {
    config(None, 10 * 1024 * 1024)
}

struct Capture {
    ring: Arc<EventRing>,
    misses: RedactionMisses,
}

impl Capture {
    fn raw_lines(&self) -> Vec<String> {
        self.ring.snapshot().iter().map(|l| l.to_string()).collect()
    }

    fn lines(&self) -> Vec<Value> {
        self.raw_lines()
            .iter()
            .map(|l| serde_json::from_str(l.trim_end()).unwrap())
            .collect()
    }

    fn only_raw(&self) -> String {
        let lines = self.raw_lines();
        assert_eq!(lines.len(), 1, "{lines:#?}");
        lines[0].clone()
    }

    fn only(&self) -> Value {
        serde_json::from_str(self.only_raw().trim_end()).unwrap()
    }

    fn only_fields(&self) -> Map<String, Value> {
        self.only()["fields"].as_object().unwrap().clone()
    }
}

fn capture_with(config: LogConfig, redactor: Arc<Redactor>, emit: impl FnOnce()) -> Capture {
    let ring = Arc::new(EventRing::new(EventRing::DEFAULT_BUDGET));
    let layer = LogLayer::new(config, redactor, Arc::clone(&ring)).unwrap();
    let misses = layer.misses();
    tracing::subscriber::with_default(Registry::default().with(layer), emit);
    Capture { ring, misses }
}

fn capture(emit: impl FnOnce()) -> Capture {
    capture_with(ring_only(), redactor(), emit)
}
