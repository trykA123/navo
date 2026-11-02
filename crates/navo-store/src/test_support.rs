use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

static OPENS: LazyLock<Mutex<HashMap<PathBuf, usize>>> = LazyLock::new(Mutex::default);

pub(crate) fn record_open(path: &Path) {
    *OPENS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .entry(path.to_owned())
        .or_default() += 1;
}

#[must_use]
pub fn connection_opens(path: &Path) -> usize {
    OPENS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(path)
        .copied()
        .unwrap_or_default()
}
