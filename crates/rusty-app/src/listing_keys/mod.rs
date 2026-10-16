use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use rusty_cache::{CacheKeys, DriveIdentity, DriveProbe};
use rusty_shell::VolumeIdentity;

const PROBE_TIMEOUT: Duration = Duration::from_millis(50);
const REREAD_EVERY: Duration = Duration::from_secs(30);

type Known = HashMap<char, (Instant, Option<DriveIdentity>)>;

pub fn cache_keys() -> CacheKeys {
    let probe: DriveProbe = Arc::new(|letter| {
        rusty_shell::volume_identity(letter, PROBE_TIMEOUT).map(|identity| match identity {
            VolumeIdentity::Serial(serial) => DriveIdentity::Serial(serial),
            VolumeIdentity::Remote(remote) => DriveIdentity::Remote(remote),
        })
    });
    CacheKeys::new(remembered(probe, REREAD_EVERY))
}

fn remembered(probe: DriveProbe, every: Duration) -> DriveProbe {
    let known = Mutex::new(Known::new());
    Arc::new(move |letter| {
        let letter = letter.to_ascii_uppercase();
        let lock = || known.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((read_at, identity)) = lock().get(&letter)
            && read_at.elapsed() < every
        {
            return identity.clone();
        }
        let identity = probe(letter);
        lock().insert(letter, (Instant::now(), identity.clone()));
        identity
    })
}

#[cfg(test)]
mod tests;
