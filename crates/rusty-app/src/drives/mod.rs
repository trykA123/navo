use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_shell::{Drive, DriveKind, Space};
use rusty_ui::window_view::{DriveChanges, DriveHooks, DriveIcon, DriveInfo};

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const REREAD_EVERY: Duration = Duration::from_secs(30);

pub type Details = (Option<String>, Option<Space>);
type Probe<'a> = dyn Fn(&str) -> Option<Details> + Sync + 'a;

struct Known {
    kind: DriveKind,
    remote: Option<String>,
    details: Details,
    read_at: Instant,
}

#[derive(Default)]
pub struct DriveCache {
    known: HashMap<String, Known>,
}

fn every(kind: DriveKind) -> Duration {
    match kind {
        DriveKind::Removable | DriveKind::Optical => Duration::ZERO,
        _ => REREAD_EVERY,
    }
}

impl DriveCache {
    pub fn refresh(
        &mut self,
        drives: Vec<Drive>,
        probe: &Probe<'_>,
        now: Instant,
    ) -> Vec<DriveInfo> {
        self.known
            .retain(|root, _| drives.iter().any(|drive| drive.root == *root));
        let stale: Vec<&Drive> = drives
            .iter()
            .filter(|drive| drive.connected && self.is_stale(drive, now))
            .collect();
        let read: Vec<(usize, Option<Details>)> = std::thread::scope(|scope| {
            let jobs: Vec<_> = stale
                .iter()
                .enumerate()
                .map(|(at, drive)| scope.spawn(move || (at, probe(&drive.root))))
                .collect();
            jobs.into_iter().filter_map(|job| job.join().ok()).collect()
        });
        for (at, details) in read {
            if let (Some(drive), Some(details)) = (stale.get(at), details) {
                let known = Known {
                    kind: drive.kind,
                    remote: drive.remote.clone(),
                    details,
                    read_at: now,
                };
                self.known.insert(drive.root.clone(), known);
            }
        }
        drives
            .into_iter()
            .filter_map(|drive| {
                let details = self.details_of(&drive);
                info_of(drive, details)
            })
            .collect()
    }

    fn is_stale(&self, drive: &Drive, now: Instant) -> bool {
        self.known.get(&drive.root).is_none_or(|known| {
            known.kind != drive.kind
                || known.remote != drive.remote
                || now.duration_since(known.read_at) >= every(drive.kind)
        })
    }

    fn details_of(&self, drive: &Drive) -> Details {
        self.known
            .get(&drive.root)
            .filter(|known| known.kind == drive.kind && known.remote == drive.remote)
            .map(|known| known.details.clone())
            .unwrap_or_default()
    }
}

fn icon_of(kind: DriveKind) -> DriveIcon {
    match kind {
        DriveKind::Fixed => DriveIcon::Fixed,
        DriveKind::Removable => DriveIcon::Removable,
        DriveKind::Network => DriveIcon::Network,
        DriveKind::Optical => DriveIcon::Optical,
        DriveKind::Ram => DriveIcon::Ram,
        DriveKind::Unknown => DriveIcon::Unknown,
    }
}

fn info_of(drive: Drive, (label, space): Details) -> Option<DriveInfo> {
    Some(DriveInfo {
        root: Location::new(drive.root).ok()?,
        letter: drive.letter,
        icon: icon_of(drive.kind),
        label: label.or(drive.label),
        remote: drive.remote,
        connected: drive.connected,
        space: space.map(|space| (space.total, space.free)),
    })
}

pub fn hooks() -> DriveHooks {
    let cache = Arc::new(Mutex::new(DriveCache::default()));
    DriveHooks {
        refresh: Arc::new(move || {
            let drives = rusty_shell::list_drives();
            let probe = |root: &str| rusty_shell::drive_details(root, PROBE_TIMEOUT);
            cache
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .refresh(drives, &probe, Instant::now())
        }),
        reconnect: Arc::new(rusty_shell::reconnect),
        changes: changes(),
    }
}

fn changes() -> Option<DriveChanges> {
    let (signal, changes) = DriveChanges::channel();
    let watch = rusty_shell::watch_volumes(move || signal.send())?;
    Some(changes.keeping(Box::new(watch)))
}

#[cfg(test)]
mod tests;
