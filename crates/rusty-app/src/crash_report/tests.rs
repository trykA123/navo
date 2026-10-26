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
        caches: rusty_diag::CacheRegistry::default(),
    }
}

fn unclean(data: &Path, folder: Option<&Path>) -> CrashReport {
    Running::start(data).unwrap();
    CrashReport {
        pending: pending(data),
        recovery: None,
        started: Instant::now(),
        diagnostics: diagnostics(),
        folder: folder.map(Path::to_path_buf),
    }
}

#[test]
fn save_after_restart_and_clean_quit_shows_the_notice_only_once() {
    let data = tempfile::tempdir().unwrap();
    let downloads = tempfile::tempdir().unwrap();
    let report = unclean(data.path(), Some(downloads.path()));
    let running = Running::start(data.path()).unwrap();
    let zip = report.save().unwrap();
    assert_eq!(zip.parent(), path_location(downloads.path()));
    let saved: Vec<_> = std::fs::read_dir(downloads.path()).unwrap().collect();
    assert_eq!(saved.len(), 1);
    assert!(data.path().join("running").is_file());
    running.finish().unwrap();
    assert!(pending(data.path()).is_none());
}

#[test]
fn dismiss_after_restart_and_clean_quit_shows_the_notice_only_once() {
    let data = tempfile::tempdir().unwrap();
    let report = unclean(data.path(), None);
    let running = Running::start(data.path()).unwrap();
    report.dismiss().unwrap();
    assert!(data.path().join("running").is_file());
    running.finish().unwrap();
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
        recovery: None,
        started: Instant::now(),
        diagnostics: diagnostics(),
    };
    assert!(start.service().is_none());
}

fn recovered(data: &Path, downloads: &Path) -> CrashReport {
    std::fs::write(data.join("navo.db"), [0x5a; 4096]).unwrap();
    let mut start = crate::crash::store::check_store(Some(data), true, true);
    start.share();
    CrashReport {
        pending: None,
        recovery: start.notice.clone(),
        started: Instant::now(),
        diagnostics: diagnostics(),
        folder: Some(downloads.to_owned()),
    }
}

#[test]
fn saving_a_recovery_only_report_keeps_the_persistent_notice() {
    let data = tempfile::tempdir().unwrap();
    let downloads = tempfile::tempdir().unwrap();
    let report = recovered(data.path(), downloads.path());
    report.save().unwrap();
    assert!(report.recovery().is_some());
    let mut next = crate::crash::store::check_store(Some(data.path()), false, true);
    assert!(next.notice.is_some());
    next.share();
    report.dismiss().unwrap();
    drop(next);
    assert!(
        crate::crash::store::check_store(Some(data.path()), false, true)
            .notice
            .is_none()
    );
}

#[cfg(unix)]
#[test]
fn a_failed_notice_write_returns_an_error_and_preserves_the_record() {
    use std::os::unix::fs::PermissionsExt;

    let data = tempfile::tempdir().unwrap();
    let downloads = tempfile::tempdir().unwrap();
    let report = recovered(data.path(), downloads.path());
    let path = data.path().join("store-recovery.json");
    let before = std::fs::read(&path).unwrap();
    let permissions = std::fs::metadata(data.path()).unwrap().permissions();
    std::fs::set_permissions(data.path(), std::fs::Permissions::from_mode(0o500)).unwrap();
    let result = report.dismiss();
    std::fs::set_permissions(data.path(), permissions).unwrap();
    assert!(result.is_err());
    assert_eq!(std::fs::read(path).unwrap(), before);
}
