use std::time::{Duration, Instant};

use rusty_index::pipe::{Client, FolderTotals};

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "IX8 backend API precedes UI integration")
)]
pub fn folder_totals(paths: &[String]) -> Option<Vec<Option<FolderTotals>>> {
    Client::folder_totals(paths, Instant::now() + Duration::from_millis(40)).ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn oversized_request_returns_none_before_connecting() {
        assert!(crate::search_hooks::folder_totals(&vec!["C:".into(); 65]).is_none());
    }
    #[cfg(not(windows))]
    #[test]
    fn absent_service_returns_none_within_budget() {
        let started = std::time::Instant::now();
        assert!(crate::search_hooks::folder_totals(&["C:\\folder".into()]).is_none());
        assert!(started.elapsed() < std::time::Duration::from_millis(50));
    }
}
