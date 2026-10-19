use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use rusty_diag::{EventRing, Identity, Redactor, Salt};
use rusty_ui::window_view::CrashReportService;

use super::CrashReport;
use crate::crash::{Running, pending};
use crate::logging::{Diagnostics, Opened};
use crate::startup::path_location;

fn diagnostics() -> Diagnostics {
    Diagnostics {
        redactor: Arc::new(Redactor::new(
            Salt::from_bytes([7; 16]),
            Identity::default(),
        )),
        ring: Arc::new(EventRing::new(EventRing::DEFAULT_BUDGET)),
        misses: Opened::default().misses(),
    }
}

fn unclean(data: &Path, folder: Option<&Path>) -> CrashReport {
    Running::start(data).unwrap();
    CrashReport {
        pending: pending(data).unwrap(),
        started: Instant::now(),
        diagnostics: diagnostics(),
        folder: folder.map(Path::to_path_buf),
    }
}

#[test]
fn save_writes_the_zip_in_the_folder_and_clears_the_pending_crash() {
    let data = tempfile::tempdir().unwrap();
    let downloads = tempfile::tempdir().unwrap();
    let report = unclean(data.path(), Some(downloads.path()));
    let zip = report.save().unwrap();
    assert_eq!(zip.parent(), path_location(downloads.path()));
    let saved: Vec<_> = std::fs::read_dir(downloads.path()).unwrap().collect();
    assert_eq!(saved.len(), 1);
    assert!(pending(data.path()).is_none());
}

#[test]
fn dismiss_clears_the_pending_crash() {
    let data = tempfile::tempdir().unwrap();
    let report = unclean(data.path(), None);
    report.dismiss();
    assert!(pending(data.path()).is_none());
}

#[test]
fn save_without_a_folder_reports_it_and_keeps_the_pending_crash() {
    let data = tempfile::tempdir().unwrap();
    let report = unclean(data.path(), None);
    assert_eq!(
        report.save().unwrap_err(),
        "No Downloads or home folder was found"
    );
    assert!(pending(data.path()).is_some());
}

#[test]
fn no_service_after_a_clean_start() {
    let start = super::CrashStart {
        pending: None,
        started: Instant::now(),
        diagnostics: diagnostics(),
    };
    assert!(start.service().is_none());
}
