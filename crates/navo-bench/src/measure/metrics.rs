use serde_json::Value;

const FIRST_FRAME: &str = "first frame";
const FIRST_USABLE: &str = "first usable";
/// A start slower than this is a broken number, not a measurement.
const MAX_MS: f64 = 3_600_000.0;

/// Why a `--metrics` file gave no numbers. Carries a kind only, never input text.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum MetricsError {
    #[error("metrics:unreadable")]
    Json,
    #[error("metrics:no-start-numbers")]
    Empty,
}

/// The two start numbers of a tester build, in milliseconds since the process began.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StartMetrics {
    pub first_frame_ms: Option<f64>,
    pub first_usable_ms: Option<f64>,
}

/// Reads the numbers of `navo --metrics`; every string in the file is dropped unread.
/// Fields come first, then the marks named `first frame` and `first usable`.
pub fn parse_metrics(json: &str) -> Result<StartMetrics, MetricsError> {
    let Ok(Value::Object(raw)) = serde_json::from_str::<Value>(json) else {
        return Err(MetricsError::Json);
    };
    let marks = raw.get("marks").unwrap_or(&Value::Null);
    let metrics = StartMetrics {
        first_frame_ms: number(raw.get("first_frame_ms")).or_else(|| mark(marks, FIRST_FRAME)),
        first_usable_ms: number(raw.get("first_usable_ms")).or_else(|| mark(marks, FIRST_USABLE)),
    };
    if metrics.first_frame_ms.is_none() && metrics.first_usable_ms.is_none() {
        return Err(MetricsError::Empty);
    }
    Ok(metrics)
}

fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|ms| ms.is_finite() && (0.0..=MAX_MS).contains(ms))
}

fn mark(marks: &Value, name: &str) -> Option<f64> {
    marks
        .as_array()?
        .iter()
        .find(|entry| entry.get("name").and_then(Value::as_str) == Some(name))
        .and_then(|entry| number(entry.get("ms")))
}
