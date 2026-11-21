//! Durable copies for the app (P1 A1): opens the write journal once at start, keeps the copy
//! host for the process, and words the start-up toast (recovery note or "Copying is off").

use std::path::Path;
use std::sync::{Arc, OnceLock};

use navo_compare_profile::book::profile_text;
use navo_compare_profile::paths::open_profile;
use sidewise_fs::durable::RealDisk;
use sidewise_host::compare::diff::write::host::LocalCopyHost;
use sidewise_journal::{Journal, Recovered};
use sidewise_kit::prelude::App;
use sidewise_l10n::t;

use crate::layers::toast;

/// The process copy host; unset when the journal did not open, so copies stay refused (NC142).
static HOST: OnceLock<Arc<LocalCopyHost>> = OnceLock::new();

/// Opens the journal in the profile (`explicit` folder, else the platform folders) and installs
/// the copy host. `Ok(Some(note))` when recovery rolled back, kept or failed anything;
/// `Err("Copying is off: <reason>")` when the profile or the journal did not open (fail closed).
pub(crate) fn install_writes(explicit: Option<&Path>) -> Result<Option<String>, String> {
    if HOST.get().is_some() {
        return Ok(None);
    }
    let (host, note) = open_writes(explicit)?;
    // A lost race keeps the first host; this one drops and releases its owner lock.
    let _ = HOST.set(host);
    Ok(note)
}

/// The copy host, when the journal opened at start.
pub(crate) fn host() -> Option<Arc<LocalCopyHost>> {
    HOST.get().cloned()
}

/// Opens the profile and its journal (recovery and retention sweep run here) and returns a new
/// copy host with the recovery note; fails with "Copying is off: <reason>".
pub(crate) fn open_writes(
    explicit: Option<&Path>,
) -> Result<(Arc<LocalCopyHost>, Option<String>), String> {
    let profile = open_profile(explicit).map_err(|error| off(&profile_text(&error)))?;
    let dir = profile.journal_dir();
    let (journal, recovered) = Journal::open(&dir, Arc::new(RealDisk)).map_err(|error| {
        tracing::warn!(dir = %dir.display(), %error, "journal did not open; copying is off");
        off(&error.to_string())
    })?;
    if recovered.swept.failed > 0 {
        tracing::warn!(
            failed = recovered.swept.failed,
            bytes_left = recovered.swept.bytes_left,
            "journal retention sweep left jobs for the next start"
        );
    }
    let host = Arc::new(LocalCopyHost::new(Arc::new(journal)));
    Ok((host, recovery_note(&recovered, &dir)))
}

/// Toasts the outcome of `install_writes` once the shell is up: the recovery note or the
/// "Copying is off" reason; nothing for a clean start. With no toast host, prints it instead.
pub(crate) fn announce(outcome: Result<Option<String>, String>, cx: &mut App) {
    if let Ok(Some(text)) | Err(text) = outcome
        && !toast::notify(text.clone(), cx)
    {
        report(&text);
    }
}

/// Prints a start-up note on the terminal when no window can show it.
#[expect(
    clippy::print_stderr,
    reason = "start-up notes go to the terminal without a window"
)]
fn report(text: &str) {
    eprintln!("{}: {text}", navo_compare_profile::brand::SLUG);
}

/// "Copying is off: <reason>"; the reason text is not localised yet.
fn off(reason: &str) -> String {
    t!("compare-writes-off", reason = reason)
}

/// The recovery toast: copies and undos rolled back, files restored after each, what was left,
/// old backups the sweep could not remove, and the journal folder that keeps the backups.
/// `None` when there is nothing to report.
pub(crate) fn recovery_note(recovered: &Recovered, dir: &Path) -> Option<String> {
    let copies = recovered
        .rolled_back
        .saturating_sub(recovered.rolled_back_undos);
    let copy_files = recovered.restored.saturating_sub(recovered.restored_undos);
    let (undos, undo_files) = (recovered.rolled_back_undos, recovered.restored_undos);
    let counts = [
        (
            "compare-recovery-rolled-back",
            shown_without(copies, copy_files),
        ),
        ("compare-recovery-restored", copy_files),
        (
            "compare-recovery-undo-rolled-back",
            shown_without(undos, undo_files),
        ),
        ("compare-recovery-restored-undo", undo_files),
        ("compare-recovery-kept", recovered.kept),
        ("compare-recovery-failed", recovered.failed),
        ("compare-recovery-damaged", recovered.damaged),
        ("compare-recovery-sweep-failed", recovered.swept.failed),
    ];
    let mut parts: Vec<String> = counts
        .iter()
        .filter(|(_, count)| *count > 0)
        .map(|(id, count)| t!(id, count = *count))
        .collect();
    if counts[4..].iter().any(|(_, count)| *count > 0) {
        let folder = dir.display().to_string();
        parts.push(t!("compare-recovery-folder", folder = folder));
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// The rolled-back job count to show: `jobs`, or 0 when `restored` files were counted, since
/// the restored line already says those jobs were rolled back.
fn shown_without(jobs: u32, restored: u32) -> u32 {
    if restored == 0 { jobs } else { 0 }
}
