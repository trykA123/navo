use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use rusty_core::Location;
use rusty_fs::volume_of;

use crate::CacheKey;

const REREAD_EVERY: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveIdentity {
    Serial(u32),
    Remote(String),
}

pub type DriveProbe = Arc<dyn Fn(char) -> Option<DriveIdentity> + Send + Sync>;

#[derive(Default)]
struct Drives {
    known: HashMap<char, (Instant, DriveIdentity)>,
    probing: HashSet<char>,
    started: usize,
}

#[derive(Clone)]
pub struct CacheKeys {
    probe: DriveProbe,
    drives: Arc<Mutex<Drives>>,
}

impl Default for CacheKeys {
    fn default() -> Self {
        Self::new(Arc::new(|_| None))
    }
}

impl CacheKeys {
    #[must_use]
    pub fn new(probe: DriveProbe) -> Self {
        Self {
            probe,
            drives: Arc::default(),
        }
    }

    #[must_use]
    pub fn key(&self, location: &Location) -> Option<CacheKey> {
        let volume = volume_of(location);
        let root = volume.as_str();
        let serial = match drive_letter(root) {
            Some(letter) => match self.drive(letter.to_ascii_uppercase())? {
                DriveIdentity::Serial(serial) => u64::from(serial),
                DriveIdentity::Remote(remote) => fnv1a(&remote.to_lowercase()),
            },
            None => fnv1a(root),
        };
        Some(CacheKey::new(location.clone(), Some(serial)))
    }

    #[must_use]
    pub fn probes_started(&self) -> usize {
        lock(&self.drives).started
    }

    fn drive(&self, letter: char) -> Option<DriveIdentity> {
        let mut drives = lock(&self.drives);
        let known = drives.known.get(&letter).cloned();
        if known
            .as_ref()
            .is_none_or(|(read_at, _)| read_at.elapsed() >= REREAD_EVERY)
        {
            self.start_probe(&mut drives, letter);
        }
        known.map(|(_, identity)| identity)
    }

    fn start_probe(&self, drives: &mut Drives, letter: char) {
        if !drives.probing.insert(letter) {
            return;
        }
        drives.started += 1;
        let (probe, shared) = (Arc::clone(&self.probe), Arc::clone(&self.drives));
        let spawned = std::thread::Builder::new()
            .name("volume-probe".to_owned())
            .spawn(move || {
                let identity = probe(letter);
                let mut drives = lock(&shared);
                drives.probing.remove(&letter);
                if let Some(identity) = identity {
                    drives.known.insert(letter, (Instant::now(), identity));
                }
            });
        if spawned.is_err() {
            drives.probing.remove(&letter);
        }
    }
}

fn lock(drives: &Mutex<Drives>) -> MutexGuard<'_, Drives> {
    drives.lock().unwrap_or_else(PoisonError::into_inner)
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
