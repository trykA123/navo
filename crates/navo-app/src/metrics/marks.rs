use std::time::Instant;

#[cfg(feature = "tester")]
static MARKS: std::sync::Mutex<Vec<(&'static str, Instant)>> = std::sync::Mutex::new(Vec::new());

#[cfg(feature = "tester")]
pub(crate) fn mark(label: &'static str) {
    if let Ok(mut marks) = MARKS.lock() {
        marks.push((label, Instant::now()));
    }
}

#[cfg(not(feature = "tester"))]
pub(crate) fn mark(_label: &'static str) {}

#[cfg(feature = "tester")]
pub(crate) fn all() -> Vec<(&'static str, Instant)> {
    MARKS.lock().map(|marks| marks.clone()).unwrap_or_default()
}

#[cfg(not(feature = "tester"))]
pub(crate) fn all() -> Vec<(&'static str, Instant)> {
    Vec::new()
}
