use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
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
    answers: u64,
}

pub(crate) enum Lookup {
    Found(CacheKey),
    Probing,
    Missing,
}

#[derive(Default)]
struct Shared {
    drives: Mutex<Drives>,
    answered: Condvar,
}

#[derive(Clone)]
pub struct CacheKeys {
    probe: DriveProbe,
    shared: Arc<Shared>,
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
            shared: Arc::default(),
        }
    }

    #[must_use]
    pub fn key(&self, location: &Location) -> Option<CacheKey> {
        let volume = volume_of(location);
        let root = volume.as_str();
        let serial = match drive_letter(root) {
            Some(letter) => serial_of(&self.drive(letter.to_ascii_uppercase())?),
            None => fnv1a(root),
        };
        Some(CacheKey::new(location.clone(), Some(serial)))
    }

    #[must_use]
    pub fn key_within(&self, location: &Location, deadline: Instant) -> Option<CacheKey> {
        if let Some(key) = self.key(location) {
            return Some(key);
        }
        loop {
            let answers = self.answers();
            match self.lookup(location) {
                Lookup::Found(key) => return Some(key),
                Lookup::Missing => return None,
                Lookup::Probing if !self.wait_for_answer(answers, deadline) => return None,
                Lookup::Probing => {}
            }
        }
    }

    pub(crate) fn lookup(&self, location: &Location) -> Lookup {
        let volume = volume_of(location);
        let Some(letter) = drive_letter(volume.as_str()) else {
            return Lookup::Found(CacheKey::new(
                location.clone(),
                Some(fnv1a(volume.as_str())),
            ));
        };
        let letter = letter.to_ascii_uppercase();
        let drives = self.lock();
        match drives.known.get(&letter) {
            Some((_, identity)) => {
                Lookup::Found(CacheKey::new(location.clone(), Some(serial_of(identity))))
            }
            None if drives.probing.contains(&letter) => Lookup::Probing,
            None => Lookup::Missing,
        }
    }

    pub(crate) fn answers(&self) -> u64 {
        self.lock().answers
    }

    pub(crate) fn wait_for_answer(&self, seen: u64, deadline: Instant) -> bool {
        let drives = self.lock();
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            return false;
        };
        let (drives, _) = self
            .shared
            .answered
            .wait_timeout_while(drives, left, |drives| drives.answers == seen)
            .unwrap_or_else(PoisonError::into_inner);
        drives.answers != seen
    }

    #[must_use]
    pub fn is_network(&self, location: &Location) -> bool {
        if !location.is_local() {
            return false;
        }
        let volume = volume_of(location);
        match drive_letter(volume.as_str()) {
            Some(letter) => matches!(
                self.drive(letter.to_ascii_uppercase()),
                Some(DriveIdentity::Remote(_))
            ),
            None => volume.as_str().starts_with(r"\\"),
        }
    }

    #[must_use]
    pub fn probes_started(&self) -> usize {
        self.lock().started
    }

    fn drive(&self, letter: char) -> Option<DriveIdentity> {
        let mut drives = self.lock();
        let known = drives.known.get(&letter).cloned();
        if known
            .as_ref()
            .is_none_or(|(read_at, _)| read_at.elapsed() >= REREAD_EVERY)
        {
            self.start_probe(&mut drives, letter);
        }
        known.map(|(_, identity)| identity)
    }

    fn lock(&self) -> MutexGuard<'_, Drives> {
        lock(&self.shared.drives)
    }

    fn start_probe(&self, drives: &mut Drives, letter: char) {
        if !drives.probing.insert(letter) {
            return;
        }
        drives.started += 1;
        let (probe, shared) = (Arc::clone(&self.probe), Arc::clone(&self.shared));
        let spawned = std::thread::Builder::new()
            .name("volume-probe".to_owned())
            .spawn(move || {
                let identity = probe(letter);
                let mut drives = lock(&shared.drives);
                drives.probing.remove(&letter);
                drives.answers += 1;
                if let Some(identity) = identity {
                    drives.known.insert(letter, (Instant::now(), identity));
                }
                shared.answered.notify_all();
            });
        if spawned.is_err() {
            drives.probing.remove(&letter);
        }
    }
}

fn lock(drives: &Mutex<Drives>) -> MutexGuard<'_, Drives> {
    drives.lock().unwrap_or_else(PoisonError::into_inner)
}

fn serial_of(identity: &DriveIdentity) -> u64 {
    match identity {
        DriveIdentity::Serial(serial) => u64::from(*serial),
        DriveIdentity::Remote(remote) => fnv1a(&remote.to_lowercase()),
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
