//! Bulk insert for the fake provider: builds a large folder in one pass for benches and tests.

use super::{FakeFs, normalize};
use crate::{FsEntry, WatchChange};

impl FakeFs {
    /// Adds many entries to `folder` in O(n) and gives the same tree as `add_entry` per entry.
    /// Precondition: names are unique inside the folder (compared folded on case-folding
    /// providers) and not already present. Panics on a duplicate name.
    #[expect(
        clippy::panic,
        reason = "a duplicate name breaks the documented precondition"
    )]
    pub fn add_entries(&self, folder: &str, entries: impl IntoIterator<Item = FsEntry>) {
        let base = normalize(folder);
        let missing = self.missing_folders(&format!("{base}/x"));
        let entries: Vec<FsEntry> = entries.into_iter().collect();
        let paths: Vec<String> = entries
            .iter()
            .map(|e| format!("{base}/{}", e.name.to_text()))
            .collect();
        if let Err(name) = self.tree().insert_all(folder, entries) {
            panic!("add_entries: duplicate name {name:?} in {folder:?}");
        }
        self.announce(&missing);
        for path in paths {
            self.changed(&path, WatchChange::Created);
        }
    }
}
