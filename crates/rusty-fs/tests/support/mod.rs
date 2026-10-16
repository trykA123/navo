#![allow(dead_code, reason = "each test binary uses a part of these helpers")]

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use rusty_core::{Location, ProviderPath};
use rusty_fs::{LocalFs, Provider, Watch, WatchChange, WatchEvent};
use tempfile::TempDir;

pub const PATIENCE: Duration = Duration::from_secs(20);
pub const PROBE: &str = "probe.tmp";

#[cfg(unix)]
pub fn location_of(path: &Path) -> Location {
    use std::os::unix::ffi::OsStrExt;
    Location::local(ProviderPath::from_bytes(path.as_os_str().as_bytes())).unwrap()
}

#[cfg(windows)]
pub fn location_of(path: &Path) -> Location {
    use std::os::windows::ffi::OsStrExt;
    let units: Vec<u16> = path.as_os_str().encode_wide().collect();
    Location::local(ProviderPath::from_wide(&units)).unwrap()
}

pub fn name(text: &str) -> ProviderPath {
    ProviderPath::from_text(text)
}

pub struct Watched {
    pub dir: TempDir,
    pub watch: Option<Watch>,
    pub events: Receiver<WatchEvent>,
}

impl Watched {
    pub fn path(&self, child: &str) -> PathBuf {
        self.dir.path().join(child)
    }
}

pub fn watch_path(path: &Path) -> (Watch, Receiver<WatchEvent>) {
    let (watch, events) = start_watch(path);
    wait_until_live(path, &events);
    (watch, events)
}

pub fn wait_until_live(path: &Path, events: &Receiver<WatchEvent>) {
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        std::fs::write(path.join(PROBE), b"x").unwrap();
        match events.recv_timeout(Duration::from_millis(250)) {
            Ok(WatchEvent::Changed(_)) => return,
            Ok(other) => panic!("the watch did not start: {other:?}"),
            Err(_) => {}
        }
    }
    panic!("the watch never went live");
}

pub fn start_watch(path: &Path) -> (Watch, Receiver<WatchEvent>) {
    let (sender, events) = mpsc::channel();
    let watch = LocalFs::new()
        .watch(
            &location_of(path),
            Box::new(move |event| {
                let _ = sender.send(event);
            }),
        )
        .unwrap();
    (watch, events)
}

pub fn watched_with(prepare: impl FnOnce(&Path)) -> Watched {
    let dir = tempfile::tempdir().unwrap();
    prepare(dir.path());
    let (watch, events) = watch_path(dir.path());
    Watched {
        dir,
        watch: Some(watch),
        events,
    }
}

pub fn watched() -> Watched {
    watched_with(|_| {})
}

pub struct Seen {
    pub changes: Vec<WatchChange>,
    pub batches: usize,
    pub overflowed: bool,
}

pub fn wait_for(events: &Receiver<WatchEvent>, mut done: impl FnMut(&Seen) -> bool) -> Seen {
    let deadline = Instant::now() + PATIENCE;
    let mut seen = Seen {
        changes: Vec::new(),
        batches: 0,
        overflowed: false,
    };
    while !done(&seen) {
        let left = deadline.saturating_duration_since(Instant::now());
        match events.recv_timeout(left) {
            Ok(WatchEvent::Changed(changes)) => seen.changes.extend(changes),
            Ok(WatchEvent::Overflow) => seen.overflowed = true,
            Ok(WatchEvent::Failed(error)) => panic!("the watch failed: {error:?}"),
            Err(_) => panic!("no matching change within {PATIENCE:?}"),
        }
        seen.batches += 1;
    }
    seen
}

pub fn wait_for_change(events: &Receiver<WatchEvent>, wanted: &WatchChange) -> Seen {
    wait_for(events, |seen| seen.changes.contains(wanted))
}
