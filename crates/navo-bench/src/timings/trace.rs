use serde_json::Value;

const MAX_MS: f64 = 60_000.0;
const MAX_LINES: usize = 10_000;

/// The key-to-frame numbers of a `--trace` file, one per record.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TraceNumbers {
    pub key_to_frame_ms: Vec<f64>,
}

/// Reads the JSON lines of `navo --trace`. Action names and other fields are dropped unread,
/// and a half-written last line is skipped.
pub fn parse_trace(text: &str) -> TraceNumbers {
    let key_to_frame_ms = text
        .lines()
        .take(MAX_LINES)
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|record| record.get("key_to_frame_ms")?.as_f64())
        .filter(|ms| ms.is_finite() && (0.0..=MAX_MS).contains(ms))
        .collect();
    TraceNumbers { key_to_frame_ms }
}
