use std::path::Path;
use std::sync::Arc;

use rusty_ui::window_view::SettingsHooks;

const SHOWN: &str = "guide-hint-shown";

pub(crate) fn wire(hooks: &mut SettingsHooks, dirs: Option<&rusty_store::StoreDirs>) {
    let marker = dirs.map(|dirs| dirs.data.join(SHOWN));
    hooks.take_guide_hint = Arc::new(move || marker.as_deref().is_some_and(take));
}

fn take(marker: &Path) -> bool {
    if marker.try_exists().unwrap_or(true) {
        return false;
    }
    let written = marker
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| rusty_store::write_atomic(marker, b""));
    match written {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(%error, "cannot remember the user guide hint");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::take;

    #[test]
    fn the_hint_is_due_once_per_profile() {
        let dir = tempfile::tempdir().expect("temp dir");
        let marker = dir.path().join("data").join("guide-hint-shown");
        assert!(take(&marker));
        assert!(!take(&marker));
        assert!(marker.exists());
    }
}
