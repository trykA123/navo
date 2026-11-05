use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use navo_core::{FileIdentity, Location};

use super::*;

#[derive(Clone, Default)]
struct WatchState {
    live: Arc<AtomicBool>,
    changes: Arc<AtomicU64>,
}

struct FakeWatch {
    live: Arc<AtomicBool>,
    changes: Arc<AtomicU64>,
    slow_drop: bool,
}

impl RepoWatch for FakeWatch {
    fn is_live(&self) -> bool {
        self.live.load(Ordering::SeqCst)
    }

    fn changes(&self) -> u64 {
        self.changes.load(Ordering::SeqCst)
    }
}

impl Drop for FakeWatch {
    fn drop(&mut self) {
        if self.slow_drop {
            std::thread::sleep(Duration::from_millis(500));
        }
    }
}

#[derive(Default)]
struct FakeProbe {
    identities: Mutex<HashMap<PathBuf, Option<FileIdentity>>>,
    states: Mutex<HashMap<PathBuf, WatchState>>,
    slow: Mutex<Option<PathBuf>>,
    created: AtomicUsize,
}

impl Probe for FakeProbe {
    fn has_git(&self, _: &Path) -> bool {
        false
    }

    fn drive_class(&self, _: &Location) -> DriveClass {
        DriveClass::Unknown
    }

    fn identity(&self, root: &Path) -> Option<FileIdentity> {
        self.identities.lock().unwrap().get(root).copied().flatten()
    }

    fn watch_tree(&self, root: &Path) -> Option<Box<dyn RepoWatch>> {
        self.created.fetch_add(1, Ordering::SeqCst);
        let state = self.state(root);
        let slow_drop = self.slow.lock().unwrap().as_deref() == Some(root);
        Some(Box::new(FakeWatch {
            live: state.live,
            changes: state.changes,
            slow_drop,
        }))
    }
}

impl FakeProbe {
    fn set_identity(&self, root: &Path, identity: Option<FileIdentity>) {
        self.identities
            .lock()
            .unwrap()
            .insert(root.to_path_buf(), identity);
    }

    fn state(&self, root: &Path) -> WatchState {
        self.states
            .lock()
            .unwrap()
            .entry(root.to_path_buf())
            .or_default()
            .clone()
    }

    fn make_live(&self, root: &Path) {
        self.state(root).live.store(true, Ordering::SeqCst);
    }

    fn bump(&self, root: &Path) {
        self.state(root).changes.fetch_add(1, Ordering::SeqCst);
    }

    fn slow_drop(&self, root: &Path) {
        *self.slow.lock().unwrap() = Some(root.to_path_buf());
    }

    fn created(&self) -> usize {
        self.created.load(Ordering::SeqCst)
    }
}

fn identity(number: u64) -> FileIdentity {
    FileIdentity::fingerprint(number, Some(number as i64), true)
}

fn path(text: &str) -> PathBuf {
    PathBuf::from(text)
}

fn roots(count: usize) -> Vec<PathBuf> {
    (0..count)
        .map(|index| path(&format!("repo{index}")))
        .collect()
}

#[test]
fn mark_needs_a_live_watch_and_tracks_raw_changes() {
    let probe = Arc::new(FakeProbe::default());
    let root = path("repo");
    probe.set_identity(&root, Some(identity(1)));
    let mut watches = Watches::new(probe.clone());
    watches.arm(&root, &root, DriveClass::Ssd);

    assert_eq!(
        watches.mark(&root),
        None,
        "a watch that has not started is no coverage"
    );

    probe.make_live(&root);
    let mark = watches.mark(&root).expect("a live watch covers the root");
    assert_eq!(mark.changes, 0);

    probe.bump(&root);
    let after = watches
        .mark(&root)
        .expect("a live watch still covers the root");
    assert_eq!(after.epoch, mark.epoch, "changes keep the watch's epoch");
    assert_eq!(after.changes, 1);

    probe.state(&root).live.store(false, Ordering::SeqCst);
    assert_eq!(watches.mark(&root), None, "a failed watch is no coverage");
}

#[test]
fn a_root_without_an_identity_is_never_armed() {
    let probe = Arc::new(FakeProbe::default());
    let root = path("repo");
    let mut watches = Watches::new(probe.clone());

    watches.arm(&root, &root, DriveClass::Ssd);

    assert_eq!(
        probe.created(),
        0,
        "a watch is only armed when the root has an identity"
    );
    assert_eq!(watches.mark(&root), None);
}

#[test]
fn a_changed_identity_clears_the_mark_and_re_arming_starts_a_new_epoch() {
    let probe = Arc::new(FakeProbe::default());
    let root = path("repo");
    probe.set_identity(&root, Some(identity(1)));
    probe.make_live(&root);
    let mut watches = Watches::new(probe.clone());
    watches.arm(&root, &root, DriveClass::Ssd);
    let first = watches
        .mark(&root)
        .expect("the armed watch covers the root");

    probe.set_identity(&root, Some(identity(2)));
    assert_eq!(watches.mark(&root), None, "the folder was replaced");

    watches.arm(&root, &root, DriveClass::Ssd);
    let second = watches.mark(&root).expect("the new watch covers the root");
    assert_ne!(second.epoch, first.epoch, "a new watch has a new epoch");
}

#[test]
fn sixty_five_roots_evict_the_least_recently_armed() {
    let probe = Arc::new(FakeProbe::default());
    let roots = roots(65);
    for root in &roots {
        probe.set_identity(root, Some(identity(1)));
        probe.make_live(root);
    }
    let mut watches = Watches::new(probe);

    for root in &roots {
        watches.arm(root, root, DriveClass::Ssd);
    }

    assert_eq!(
        watches.mark(&roots[0]),
        None,
        "the least recently armed watch was evicted"
    );
    assert!(
        watches.mark(&roots[1]).is_some(),
        "only one watch was evicted"
    );
    assert!(
        watches.mark(&roots[64]).is_some(),
        "the newest watch is armed"
    );
}

#[test]
fn evicting_a_slow_watch_does_not_block_arming() {
    let probe = Arc::new(FakeProbe::default());
    let roots = roots(65);
    for root in &roots {
        probe.set_identity(root, Some(identity(1)));
        probe.make_live(root);
    }
    probe.slow_drop(&roots[0]);
    let mut watches = Watches::new(probe);

    for root in &roots[..64] {
        watches.arm(root, root, DriveClass::Ssd);
    }
    let start = Instant::now();
    watches.arm(&roots[64], &roots[64], DriveClass::Ssd);

    assert!(
        start.elapsed() < Duration::from_millis(100),
        "arming waited {:?} for the evicted watch to drop",
        start.elapsed()
    );
}

#[test]
fn only_fixed_drives_and_non_volume_roots_are_armed() {
    let probe = Arc::new(FakeProbe::default());
    let root = path("repo");
    probe.set_identity(&root, Some(identity(1)));
    let mut watches = Watches::new(probe.clone());

    for class in [
        DriveClass::Network,
        DriveClass::Removable,
        DriveClass::Unknown,
    ] {
        watches.arm(&root, &root, class);
        assert_eq!(probe.created(), 0, "{class:?} is not a fixed drive");
    }
    for class in [DriveClass::Ssd, DriveClass::Hdd] {
        watches.arm(&root, &root, class);
    }
    assert_eq!(probe.created(), 2, "both fixed classes are armed");

    let volume = if cfg!(windows) {
        Path::new(r"C:\")
    } else {
        Path::new("/")
    };
    probe.set_identity(volume, Some(identity(2)));
    watches.arm(volume, volume, DriveClass::Ssd);
    assert_eq!(probe.created(), 2, "a volume root is never armed");
}

#[test]
fn root_key_uses_forward_slashes() {
    assert_eq!(root_key(Path::new("/a\\b")), "/a/b");
}

#[cfg(windows)]
#[test]
fn root_key_is_case_insensitive_on_windows() {
    assert_eq!(root_key(Path::new("C:/R")), root_key(Path::new("c:\\r")));
}

fn armed_under(listing: &str, root: &str) -> bool {
    let probe = Arc::new(FakeProbe::default());
    let root = path(root);
    probe.set_identity(&root, Some(identity(1)));
    let mut watches = Watches::new(probe.clone());
    watches.arm(&root, &path(listing), DriveClass::Ssd);
    probe.created() == 1
}

#[test]
fn only_the_listing_its_ancestors_and_direct_children_are_armed() {
    assert!(armed_under("l/a", "l/a"), "the listing itself");
    assert!(armed_under("l/a", "l"), "an ancestor of the listing");
    assert!(armed_under("l/a/", "l/a/repo"), "a direct child");
    assert!(
        !armed_under("l/a", "l/a/sub/repo"),
        "a grandchild would block renaming its parent"
    );
    assert!(!armed_under("l/a", "l/b"), "a sibling of the listing");
    assert!(
        !armed_under("l/a", "l/ab"),
        "a name that only shares a prefix"
    );
}

#[test]
fn windows_keys_drop_the_verbatim_prefix_case_and_trailing_separator() {
    assert_eq!(
        key_of(r"\\?\C:\Users\A\", true),
        key_of(r"c:\users\a", true)
    );
    assert_eq!(key_of(r"\\?\C:\", true), "c:/");
    assert_eq!(key_of(r"C:\", true), "c:/");
    assert_eq!(key_of(r"\\?\UNC\Srv\Share\x", true), "//srv/share/x");
    assert_eq!(
        key_of(r"\\?\C:\A", false),
        "//?/C:/A",
        "only Windows strips it"
    );
}

#[test]
fn contains_respects_folder_boundaries() {
    assert!(contains("c:/a", "c:/a"));
    assert!(contains("c:/a", "c:/a/b"));
    assert!(contains("c:/", "c:/a"));
    assert!(contains("/", "/home"));
    assert!(!contains("c:/a", "c:/ab"));
    assert!(!contains("c:/a/b", "c:/a"));
}
