use std::sync::Arc;

use rusty_core::Location;
use rusty_fs::volume_of;

use crate::CacheKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveIdentity {
    Serial(u32),
    Remote(String),
}

pub type DriveProbe = Arc<dyn Fn(char) -> Option<DriveIdentity> + Send + Sync>;

#[derive(Clone)]
pub struct CacheKeys {
    probe: DriveProbe,
}

impl Default for CacheKeys {
    fn default() -> Self {
        Self::new(Arc::new(|_| None))
    }
}

impl CacheKeys {
    #[must_use]
    pub fn new(probe: DriveProbe) -> Self {
        Self { probe }
    }

    #[must_use]
    pub fn key(&self, location: &Location) -> CacheKey {
        let volume = volume_of(location);
        let root = volume.as_str();
        let serial = drive_letter(root)
            .and_then(|letter| (self.probe)(letter))
            .map_or_else(
                || fnv1a(root),
                |identity| match identity {
                    DriveIdentity::Serial(serial) => u64::from(serial),
                    DriveIdentity::Remote(remote) => fnv1a(&remote.to_lowercase()),
                },
            );
        CacheKey::new(location.clone(), Some(serial))
    }
}

fn drive_letter(root: &str) -> Option<char> {
    let mut chars = root.chars();
    match (chars.next(), chars.next(), chars.next()) {
        (Some(letter), Some(':'), None) if letter.is_ascii_alphabetic() => Some(letter),
        _ => None,
    }
}

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}
