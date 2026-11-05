use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, PoisonError};

use navo_core::{Location, ProviderPath};
use navo_ui::git::RepoBadge;

use super::queue::Shared;

/// A repository row the scanner holds a badge for.
#[derive(Debug, Clone)]
pub(crate) struct ScanRepo {
    pub listing: Location,
    pub path: PathBuf,
    pub badge: RepoBadge,
}

/// Read access to the scanner's badges and its rescan, for the fetch timer.
#[derive(Clone)]
pub(crate) struct ScanView {
    pub(super) shared: Arc<(Mutex<Shared>, Condvar)>,
}

impl ScanView {
    pub(crate) fn repos(&self) -> Vec<ScanRepo> {
        let shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        shared
            .active
            .iter()
            .filter_map(|(folder, entry)| Some((folder, entry.badges.as_ref()?)))
            .flat_map(|(folder, badges)| {
                badges
                    .by_name
                    .iter()
                    .filter(|(_, badge)| badge.is_root)
                    .filter_map(|(name, badge)| repo(folder, name, badge))
            })
            .collect()
    }

    pub(crate) fn rescan(&self, folder: &Location) {
        let mut shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        shared.rescan(folder);
        self.shared.1.notify_one();
    }

    /// Rechecks one repository's root in a listing.
    #[expect(dead_code, reason = "a fetch uses this scope in a later atom")]
    pub(crate) fn rescan_repo(&self, folder: &Location, path: PathBuf) {
        let mut shared = self.shared.0.lock().unwrap_or_else(PoisonError::into_inner);
        shared.rescan_repo(folder, path);
        self.shared.1.notify_one();
    }
}

fn repo(folder: &Location, name: &str, badge: &RepoBadge) -> Option<ScanRepo> {
    let row = folder.join(&ProviderPath::from_text(name)).ok()?;
    Some(ScanRepo {
        listing: folder.clone(),
        path: navo_fs::os_path(&row).ok()?,
        badge: badge.clone(),
    })
}
