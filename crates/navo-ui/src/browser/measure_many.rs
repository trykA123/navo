//! Totals for many folders at once: the default `query_many` asks the source once per folder.
//! A source with a batch path (the index service) overrides it; the contract stays the same.

use navo_core::Location;

use super::{FolderTotalsSource, TotalsState};

/// Asks `source` for each folder in turn; the result has the input's length and order.
pub(super) fn each<S: FolderTotalsSource + ?Sized>(
    source: &S,
    folders: &[Location],
) -> Vec<TotalsState> {
    folders.iter().map(|folder| source.query(folder)).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::super::{FolderTotals, FolderTotalsQuery};
    use super::*;

    /// Answers by folder name and counts every single-folder call.
    #[derive(Default)]
    struct Counting {
        calls: AtomicUsize,
    }

    impl FolderTotalsSource for Counting {
        fn query(&self, folder: &Location) -> TotalsState {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let path = folder.as_str();
            if path.ends_with("busy") {
                TotalsState::Busy
            } else if path.ends_with("gone") {
                TotalsState::Unavailable
            } else {
                TotalsState::Ready(FolderTotals {
                    bytes: path.len() as u64,
                    ..FolderTotals::default()
                })
            }
        }
    }

    fn folders(names: &[&str]) -> Vec<Location> {
        names
            .iter()
            .map(|name| Location::new(format!(r"C:\proj\{name}")).unwrap())
            .collect()
    }

    #[test]
    fn default_query_many_asks_once_per_folder_in_order() {
        let source = Counting::default();
        let shown = folders(&["a", "busy", "bb", "gone", "ccc"]);
        let answers = source.query_many(&shown);
        assert_eq!(source.calls.load(Ordering::SeqCst), shown.len());
        let expected: Vec<TotalsState> = shown.iter().map(|f| source.query(f)).collect();
        assert_eq!(answers, expected);
    }

    #[test]
    fn default_query_many_through_the_shared_handle_keeps_length() {
        let counting = Arc::new(Counting::default());
        let source: FolderTotalsQuery = counting.clone();
        let shown = folders(&["a", "b", "c", "d", "e", "f", "g"]);
        assert_eq!(source.query_many(&shown).len(), 7);
        assert_eq!(counting.calls.load(Ordering::SeqCst), 7);
    }

    #[test]
    fn default_query_many_with_no_folders_asks_nothing() {
        let source = Counting::default();
        assert!(source.query_many(&[]).is_empty());
        assert_eq!(source.calls.load(Ordering::SeqCst), 0);
    }
}
