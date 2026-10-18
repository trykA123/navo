use std::sync::Mutex;
use std::time::Instant;

static MARKS: Mutex<Vec<(&'static str, Instant)>> = Mutex::new(Vec::new());

pub(crate) fn mark(label: &'static str) {
    if let Ok(mut marks) = MARKS.lock() {
        marks.push((label, Instant::now()));
    }
}

pub(crate) fn all() -> Vec<(&'static str, Instant)> {
    MARKS.lock().map(|marks| marks.clone()).unwrap_or_default()
}
