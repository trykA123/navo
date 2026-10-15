use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_shell::{Drive, DriveKind, Space};
use rusty_ui::window_view::{DriveHooks, DriveIcon, DriveInfo};

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);
const NETWORK_EVERY: Duration = Duration::from_secs(30);

type Details = (Option<String>, Option<Space>);

struct Known {
    details: Details,
    read_at: Instant,
}

#[derive(Default)]
pub struct DriveCache {
    known: HashMap<String, Known>,
}

impl DriveCache {
    pub fn refresh(
        &mut self,
        drives: Vec<Drive>,
        probe: &dyn Fn(&str) -> Option<Details>,
        now: Instant,
    ) -> Vec<DriveInfo> {
        self.known
            .retain(|root, _| drives.iter().any(|drive| drive.root == *root));
        drives
            .into_iter()
            .filter_map(|drive| {
                let details = self.details_of(&drive, probe, now);
                info_of(drive, details)
            })
            .collect()
    }

    fn details_of(
        &mut self,
        drive: &Drive,
        probe: &dyn Fn(&str) -> Option<Details>,
        now: Instant,
    ) -> Details {
        let stale = self.known.get(&drive.root).is_none_or(|known| {
            drive.kind == DriveKind::Network && now.duration_since(known.read_at) >= NETWORK_EVERY
        });
        if drive.connected
            && stale
            && let Some(details) = probe(&drive.root)
        {
            self.known.insert(
                drive.root.clone(),
                Known {
                    details,
                    read_at: now,
                },
            );
        }
        self.known
            .get(&drive.root)
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
    }
}

#[cfg(test)]
mod tests;
