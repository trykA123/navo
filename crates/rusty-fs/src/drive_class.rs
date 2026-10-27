#[cfg(windows)]
mod windows;

use rusty_core::Location;

#[cfg(any(windows, test))]
use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
};

#[cfg(any(windows, test))]
use crate::VolumeId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DriveClass {
    Ssd,
    Hdd,
    Network,
    Removable,
    Unknown,
}

#[must_use]
pub fn drive_class(location: &Location) -> DriveClass {
    #[cfg(windows)]
    {
        use std::sync::LazyLock;

        static CACHE: LazyLock<Mutex<HashMap<VolumeId, DriveClass>>> =
            LazyLock::new(|| Mutex::new(HashMap::new()));

        if !location.is_local() {
            return DriveClass::Unknown;
        }
        cached_class(&CACHE, crate::volume_of(location), windows::probe)
    }
    #[cfg(not(windows))]
    {
        let _ = location;
        DriveClass::Unknown
    }
}

#[cfg(any(windows, test))]
fn cached_class(
    cache: &Mutex<HashMap<VolumeId, DriveClass>>,
    volume: VolumeId,
    probe: impl FnOnce(&str) -> DriveClass,
) -> DriveClass {
    let known = cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&volume)
        .copied();
    if let Some(class) = known {
        return class;
    }
    let class = probe(volume.as_str());
    *cache
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .entry(volume)
        .or_insert(class)
}

#[cfg(any(windows, test))]
fn classify(remote: bool, seek_penalty: Option<bool>, trim_enabled: Option<bool>) -> DriveClass {
    match (remote, seek_penalty, trim_enabled) {
        (true, _, _) => DriveClass::Network,
        (false, Some(false), _) | (false, None, Some(true)) => DriveClass::Ssd,
        (false, Some(true), _) => DriveClass::Hdd,
        (false, None, None | Some(false)) => DriveClass::Unknown,
    }
}

#[cfg(test)]
mod tests;
