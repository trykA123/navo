use std::time::{SystemTime, UNIX_EPOCH};

use navo_clipboard::{Captured, ClipService, Content, Transfer};
use navo_shell::{CaptureContent, ShellCapture};

pub(super) fn to_captured(capture: ShellCapture, at_ms: i64) -> Captured {
    let content = match capture.content {
        CaptureContent::Text(text) => Content::Text(text),
        CaptureContent::Files { paths, cut } => Content::Paths {
            paths,
            transfer: if cut { Transfer::Cut } else { Transfer::Copy },
        },
    };
    Captured {
        content,
        source_app: capture.owner_exe,
        at_ms,
    }
}

/// Hands one watcher capture to the service. The log line names the kind and size only.
pub(super) fn deliver(service: &ClipService, capture: ShellCapture) {
    let kind = capture.content.kind();
    let len = capture.content.len();
    if !service.captured(to_captured(capture, now_ms())) {
        tracing::debug!(kind, len, "clipboard capture dropped");
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| i64::try_from(elapsed.as_millis()).ok())
        .unwrap_or(0)
}
