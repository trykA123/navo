use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_index::pipe::{Client, FolderTotals};
use rusty_ui::browser::FolderTotalsQuery;

fn folder_totals(paths: &[String]) -> Option<Vec<Option<FolderTotals>>> {
    Client::folder_totals(paths, Instant::now() + Duration::from_millis(40)).ok()
}

pub fn query() -> FolderTotalsQuery {
    Arc::new(|folder| {
        if !folder.is_local() {
            return None;
        }
        folder_totals(&[folder.as_str().to_owned()])?
            .into_iter()
            .next()
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn oversized_request_returns_none_before_connecting() {
        assert!(super::folder_totals(&vec!["C:".into(); 65]).is_none());
    }
    #[cfg(not(windows))]
    #[test]
    fn absent_service_returns_none_within_budget() {
        let started = std::time::Instant::now();
        assert!(super::folder_totals(&["C:\\folder".into()]).is_none());
        assert!(started.elapsed() < std::time::Duration::from_millis(50));
    }
}
