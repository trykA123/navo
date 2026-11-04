use serde_json::Value;

use super::error::TimingError;
use super::result::SortKey;

/// A run slower than this, or a longer list, is a broken file, not a measurement.
const MAX_MS: f64 = 3_600_000.0;
const MAX_RUNS: usize = 64;
const MAX_ITEMS: usize = 64;

/// The numbers of one `navo bench --folder` file. Every string in it is dropped unread.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BenchNumbers {
    pub entries: Option<u64>,
    pub bytes: Option<u64>,
    pub list_ms: Vec<f64>,
    /// One sample list per key, in `SortKey::ALL` order.
    pub sort_ms: [Vec<f64>; 4],
    pub filter_keystroke_ms: Vec<f64>,
}

/// Reads the numbers-only JSON of `navo bench --folder`; labels are matched, never kept.
pub fn parse_bench(json: &str) -> Result<BenchNumbers, TimingError> {
    let Ok(Value::Object(raw)) = serde_json::from_str::<Value>(json) else {
        return Err(TimingError::BenchJson);
    };
    let mut numbers = BenchNumbers {
        entries: raw.get("entries").and_then(Value::as_u64),
        bytes: raw.get("bytes").and_then(Value::as_u64),
        ..BenchNumbers::default()
    };
    let runs = raw.get("runs").and_then(Value::as_array);
    for run in runs.into_iter().flatten().take(MAX_RUNS) {
        numbers.list_ms.extend(ms(run.get("list_ms")));
        numbers
            .filter_keystroke_ms
            .extend(list(run.get("filter_keystroke_ms")));
        for (slot, key) in numbers.sort_ms.iter_mut().zip(SortKey::ALL) {
            slot.extend(sort_sample(run.get("sort_ms"), key));
        }
    }
    if numbers.list_ms.is_empty() {
        return Err(TimingError::BenchJson);
    }
    Ok(numbers)
}

fn ms(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|ms| ms.is_finite() && (0.0..=MAX_MS).contains(ms))
}

fn list(value: Option<&Value>) -> Vec<f64> {
    let items = value
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    items
        .iter()
        .take(MAX_ITEMS)
        .filter_map(|item| ms(Some(item)))
        .collect()
}

fn sort_sample(sorts: Option<&Value>, key: SortKey) -> Option<f64> {
    let pairs = sorts?.as_array()?;
    pairs.iter().take(MAX_ITEMS).find_map(|pair| {
        let pair = pair.as_array()?;
        (pair.first()?.as_str()? == key.label()).then(|| ms(pair.get(1)))?
    })
}
