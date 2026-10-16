mod model;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub use model::{Drive, DriveKind, RawDrives, Space, assemble, parse_mounts};

use std::collections::HashSet;
use std::sync::{Mutex, PoisonError, mpsc};
use std::time::Duration;

static PROBING: Mutex<Option<HashSet<String>>> = Mutex::new(None);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VolumeIdentity {
    Serial(u32),
    Remote(String),
}

#[cfg(windows)]
#[must_use]
pub fn list_drives() -> Vec<Drive> {
    assemble(&windows::raw_drives())
}

#[cfg(unix)]
#[must_use]
pub fn list_drives() -> Vec<Drive> {
    let user = std::env::var("USER").unwrap_or_default();
    parse_mounts(&unix::mounts_text(), &user)
}

fn probe(root: &str) -> (Option<String>, Option<Space>) {
    #[cfg(windows)]
    {
        windows::details(root)
    }
    #[cfg(unix)]
    {
        unix::details(root)
    }
}

fn claim(root: &str) -> bool {
    let mut probing = PROBING.lock().unwrap_or_else(PoisonError::into_inner);
    probing
        .get_or_insert_with(HashSet::new)
        .insert(root.to_owned())
}

fn release(root: &str) {
    let mut probing = PROBING.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(set) = probing.as_mut() {
        set.remove(root);
    }
}

#[must_use]
pub fn drive_details(root: &str, timeout: Duration) -> Option<(Option<String>, Option<Space>)> {
    if !claim(root) {
        return None;
    }
    let (sender, receiver) = mpsc::channel();
    let owned = root.to_owned();
    let spawned = std::thread::Builder::new()
        .name("drive-probe".to_owned())
        .spawn(move || {
            let found = probe(&owned);
            release(&owned);
            let _ = sender.send(found);
        });
    if spawned.is_err() {
        release(root);
        return None;
    }
    receiver.recv_timeout(timeout).ok()
}

#[cfg(windows)]
#[must_use]
pub fn volume_identity(letter: char) -> Option<VolumeIdentity> {
    windows::identity(letter)
}

#[cfg(unix)]
#[must_use]
pub fn volume_identity(_letter: char) -> Option<VolumeIdentity> {
    None
}

#[cfg(windows)]
pub fn reconnect(letter: char, remote: &str) -> Result<(), String> {
    windows::connect(letter, remote)
}

#[cfg(unix)]
pub fn reconnect(letter: char, remote: &str) -> Result<(), String> {
    Err(format!(
        "Reconnecting {letter}: to {remote} works on Windows only."
    ))
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_assemble;
#[cfg(test)]
mod tests_platform;
