use std::time::Instant;

pub fn mark(label: &'static str) {
    let _ = label;
}

pub fn all() -> Vec<(&'static str, Instant)> {
    Vec::new()
}
