use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rusty_core::Location;

use super::support::{at, listing, names};
use crate::{CacheKey, CacheKeys, CachedListing, DiskListings, DriveIdentity, ListingCache, Warm};

const PATIENT: Duration = Duration::from_secs(5);

fn db_in(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("cache.db")
}

fn folder(text: &str) -> Location {
    Location::new(text).unwrap()
}

fn serial_keys(serial: u32) -> CacheKeys {
    CacheKeys::new(Arc::new(move |_| Some(DriveIdentity::Serial(serial))))
}

fn settled(keys: &CacheKeys, location: &Location) -> CacheKey {
    keys.key_within(location, Instant::now() + PATIENT)
        .expect("the probe answers")
}

fn warm(cache: &ListingCache, keys: &CacheKeys, folders: Vec<Location>) -> Warm {
    Warm {
        cache: cache.clone(),
        keys: keys.clone(),
        folders,
        deadline: Instant::now() + PATIENT,
    }
}

fn seed(path: &Path, keys: &CacheKeys, location: &Location, stored: CachedListing) {
    let cache = ListingCache::default();
    cache.put(settled(keys, location), stored);
    let disk = DiskListings::open(path, warm(&ListingCache::default(), keys, Vec::new()));
    disk.save_folders(&cache, keys, std::slice::from_ref(location));
    assert!(disk.flush(PATIENT));
}

fn stored_on_disk(path: &Path, keys: &CacheKeys, folders: &[Location]) -> ListingCache {
    let reread = ListingCache::default();
    let _disk = DiskListings::open(path, warm(&reread, keys, folders.to_vec()));
    reread
}

#[test]
fn key_within_waits_for_the_drive_probe() {
    let keys = serial_keys(42);
    let key = keys.key_within(&folder(r"C:\work"), Instant::now() + PATIENT);
    assert_eq!(key.and_then(|key| key.volume_serial), Some(42));
}

#[test]
fn key_within_gives_up_at_its_deadline() {
    let (_release, gate) = mpsc::channel::<()>();
    let gate = Mutex::new(gate);
    let keys = CacheKeys::new(Arc::new(move |_| {
        let _ = gate.lock().unwrap().recv();
        Some(DriveIdentity::Serial(1))
    }));
    let started = Instant::now();
    let key = keys.key_within(
        &folder(r"C:\work"),
        Instant::now() + Duration::from_millis(30),
    );
    assert!(key.is_none());
    assert!(started.elapsed() < PATIENT);
}

#[test]
fn start_loads_saved_session_folders_marked_untrusted() {
    let dir = tempfile::tempdir().unwrap();
    let work = folder(r"C:\work");
    let mut stored = listing(&["a.txt", "b.txt"]);
    stored.read_at = at(1_000);
    stored.folder_changed = Some(at(900));
    seed(&db_in(&dir), &serial_keys(42), &work, stored);

    let cache = ListingCache::default();
    let keys = serial_keys(42);
    let _disk = DiskListings::open(&db_in(&dir), warm(&cache, &keys, vec![work.clone()]));
    let loaded = cache.get(&settled(&keys, &work)).expect("loaded at start");
    assert_eq!(names(&loaded), ["a.txt", "b.txt"]);
    assert_eq!(loaded.read_at, at(1_000));
    assert_eq!(loaded.folder_changed, None);
}

#[test]
fn start_skips_a_remapped_drive() {
    let dir = tempfile::tempdir().unwrap();
    let work = folder(r"Z:\share");
    seed(
        &db_in(&dir),
        &serial_keys(1),
        &work,
        listing(&["old-share"]),
    );

    let cache = ListingCache::default();
    let keys = serial_keys(2);
    let _disk = DiskListings::open(&db_in(&dir), warm(&cache, &keys, vec![work.clone()]));
    assert!(cache.get(&settled(&keys, &work)).is_none());
}

#[test]
fn start_stops_reading_at_its_deadline() {
    let dir = tempfile::tempdir().unwrap();
    let work = folder(r"C:\work");
    seed(&db_in(&dir), &serial_keys(42), &work, listing(&["a"]));

    let cache = ListingCache::default();
    let keys = serial_keys(42);
    let late = Warm {
        deadline: Instant::now(),
        ..warm(&cache, &keys, vec![work])
    };
    let _disk = DiskListings::open(&db_in(&dir), late);
    assert_eq!(cache.stats().entries, 0);
}

#[test]
fn an_unreadable_cache_file_still_opens() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(db_in(&dir), b"definitely not sqlite, just some bytes").unwrap();
    let cache = ListingCache::default();
    let keys = serial_keys(42);
    let work = folder(r"C:\work");
    let disk = DiskListings::open(&db_in(&dir), warm(&cache, &keys, vec![work.clone()]));
    cache.put(settled(&keys, &work), listing(&["a"]));
    disk.save_folders(&cache, &keys, std::slice::from_ref(&work));
    assert!(disk.flush(PATIENT));
    drop(disk);
    assert_eq!(
        stored_on_disk(&db_in(&dir), &keys, &[work]).stats().entries,
        1
    );
}

#[test]
fn persist_writes_the_cached_listing_to_disk() {
    let dir = tempfile::tempdir().unwrap();
    let work = folder(r"C:\work");
    let keys = serial_keys(42);
    let disk = DiskListings::open(&db_in(&dir), warm(&ListingCache::default(), &keys, vec![]));
    let cache = ListingCache::default().with_disk(disk.clone());
    cache.keep_on_disk([&work]);
    let key = settled(&keys, &work);
    cache.put(key.clone(), listing(&["fresh.txt"]));
    cache.persist(&key);
    assert!(disk.flush(PATIENT));
    drop((cache, disk));

    let reread = ListingCache::default();
    let _disk = DiskListings::open(&db_in(&dir), warm(&reread, &keys, vec![work]));
    assert_eq!(names(&reread.get(&key).unwrap()), ["fresh.txt"]);
}

#[test]
fn save_folders_writes_each_cached_session_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (one, two) = (folder(r"C:\one"), folder(r"C:\two"));
    let keys = serial_keys(42);
    let cache = ListingCache::default();
    cache.put(settled(&keys, &one), listing(&["1"]));
    cache.put(settled(&keys, &two), listing(&["2"]));
    let disk = DiskListings::open(&db_in(&dir), warm(&cache, &keys, vec![]));
    disk.save_folders(&cache, &keys, &[one.clone(), two.clone()]);
    assert!(disk.flush(PATIENT));
    drop(disk);

    let reread = ListingCache::default();
    let _disk = DiskListings::open(&db_in(&dir), warm(&reread, &keys, vec![one, two]));
    assert_eq!(reread.stats().entries, 2);
}

fn stalled_x_drive() -> (CacheKeys, mpsc::Sender<()>) {
    let (release, gate) = mpsc::channel::<()>();
    let gate = Mutex::new(gate);
    let keys = CacheKeys::new(Arc::new(move |letter| {
        if letter == 'X' {
            let _ = gate.lock().unwrap().recv();
        }
        Some(DriveIdentity::Serial(42))
    }));
    (keys, release)
}

#[test]
fn key_within_does_not_restart_a_probe_that_found_nothing() {
    let keys = CacheKeys::new(Arc::new(|_| None));
    let key = keys.key_within(&folder(r"C:\work"), Instant::now() + PATIENT);
    assert!(key.is_none());
    assert_eq!(keys.probes_started(), 1);
}

#[test]
fn one_stalled_drive_does_not_starve_the_others() {
    let dir = tempfile::tempdir().unwrap();
    let work = folder(r"C:\work");
    seed(&db_in(&dir), &serial_keys(42), &work, listing(&["a"]));

    let cache = ListingCache::default();
    let (keys, _release) = stalled_x_drive();
    let start = Warm {
        deadline: Instant::now() + Duration::from_secs(1),
        ..warm(&cache, &keys, vec![folder(r"X:\offline"), work.clone()])
    };
    let _disk = DiskListings::open(&db_in(&dir), start);
    assert!(cache.get(&settled(&keys, &work)).is_some());
}

#[test]
fn warm_up_keeps_a_listing_already_in_memory() {
    let dir = tempfile::tempdir().unwrap();
    let work = folder(r"C:\work");
    let mut old = listing(&["from-disk"]);
    old.read_at = at(1_000);
    seed(&db_in(&dir), &serial_keys(42), &work, old);

    let cache = ListingCache::default();
    let keys = serial_keys(42);
    let mut fresh = listing(&["in-memory"]);
    fresh.read_at = at(1);
    cache.put(settled(&keys, &work), fresh);
    let _disk = DiskListings::open(&db_in(&dir), warm(&cache, &keys, vec![work.clone()]));
    assert_eq!(
        names(&cache.get(&settled(&keys, &work)).unwrap()),
        ["in-memory"]
    );
}

#[test]
fn a_warm_up_load_past_the_deadline_is_not_put() {
    let dir = tempfile::tempdir().unwrap();
    let work = folder(r"C:\work");
    let keys = serial_keys(42);
    seed(&db_in(&dir), &keys, &work, listing(&["a"]));
    let (db, _) = rusty_store::CacheDb::open(&db_in(&dir)).unwrap();
    let mut table = crate::disk::ListingTable::new(db, crate::DISK_BUDGET);
    let cache = ListingCache::default();
    let key = settled(&keys, &work);
    crate::disk::admit(&mut table, &cache, key.clone(), Instant::now());
    assert!(cache.get(&key).is_none());
    crate::disk::admit(&mut table, &cache, key.clone(), Instant::now() + PATIENT);
    assert!(cache.get(&key).is_some());
}

fn writing_cache(path: &Path, keys: &CacheKeys) -> (ListingCache, DiskListings) {
    let disk = DiskListings::open(path, warm(&ListingCache::default(), keys, vec![]));
    (ListingCache::default().with_disk(disk.clone()), disk)
}

fn put_and_persist(cache: &ListingCache, key: &CacheKey, stored: CachedListing) {
    cache.put(key.clone(), stored);
    cache.persist(key);
}

#[test]
fn persist_skips_a_folder_that_is_not_open() {
    let dir = tempfile::tempdir().unwrap();
    let work = folder(r"C:\work");
    let keys = serial_keys(42);
    let (cache, disk) = writing_cache(&db_in(&dir), &keys);
    cache.keep_on_disk([&folder(r"C:\other")]);
    put_and_persist(&cache, &settled(&keys, &work), listing(&["a"]));
    assert!(disk.flush(PATIENT));
    let stored = stored_on_disk(&db_in(&dir), &keys, &[work]);
    assert_eq!(stored.stats().entries, 0);
}

#[test]
fn a_folder_closed_before_its_write_is_not_written() {
    let dir = tempfile::tempdir().unwrap();
    let work = folder(r"C:\work");
    let keys = serial_keys(42);
    let (cache, disk) = writing_cache(&db_in(&dir), &keys);
    cache.keep_on_disk([&work]);
    let release = disk.stall();
    put_and_persist(&cache, &settled(&keys, &work), listing(&["a"]));
    cache.keep_on_disk([]);
    release.send(()).unwrap();
    assert!(disk.flush(PATIENT));
    let stored = stored_on_disk(&db_in(&dir), &keys, &[work]);
    assert_eq!(stored.stats().entries, 0);
}

#[test]
fn reopening_a_closed_folder_writes_it_again() {
    let dir = tempfile::tempdir().unwrap();
    let (work, other) = (folder(r"C:\work"), folder(r"C:\other"));
    let keys = serial_keys(42);
    let (cache, disk) = writing_cache(&db_in(&dir), &keys);
    let key = settled(&keys, &work);
    cache.keep_on_disk([&work]);
    put_and_persist(&cache, &key, listing(&["a"]));
    cache.keep_on_disk([&other]);
    put_and_persist(&cache, &settled(&keys, &other), listing(&["b"]));
    cache.keep_on_disk([&work]);
    let mut again = listing(&["a"]);
    again.read_at = at(2_000);
    put_and_persist(&cache, &key, again);
    assert!(disk.flush(PATIENT));
    drop((cache, disk));
    let stored = stored_on_disk(&db_in(&dir), &keys, &[work]);
    assert_eq!(stored.get(&key).unwrap().read_at, at(2_000));
}

#[test]
fn a_full_queue_drops_writes() {
    let dir = tempfile::tempdir().unwrap();
    let keys = serial_keys(42);
    let folders: Vec<Location> = (0..40).map(|i| folder(&format!(r"C:\f{i}"))).collect();
    let cache = ListingCache::default();
    folders
        .iter()
        .for_each(|location| cache.put(settled(&keys, location), listing(&["x"])));
    let disk = DiskListings::open(&db_in(&dir), warm(&ListingCache::default(), &keys, vec![]));
    let release = disk.stall();
    disk.save_folders(&cache, &keys, &folders);
    release.send(()).unwrap();
    assert!(disk.flush(PATIENT));
    drop(disk);
    let stored = stored_on_disk(&db_in(&dir), &keys, &folders)
        .stats()
        .entries;
    assert!((1..=16).contains(&stored), "{stored} rows were written");
}

#[test]
fn exit_flush_gives_up_on_a_stuck_writer_and_writes_once_it_frees() {
    let dir = tempfile::tempdir().unwrap();
    let work = folder(r"C:\work");
    let keys = serial_keys(42);
    let cache = ListingCache::default();
    cache.put(settled(&keys, &work), listing(&["a"]));
    let disk = DiskListings::open(&db_in(&dir), warm(&ListingCache::default(), &keys, vec![]));
    let release = disk.stall();
    disk.save_folders(&cache, &keys, std::slice::from_ref(&work));
    let started = Instant::now();
    assert!(!disk.flush(Duration::from_millis(50)));
    assert!(started.elapsed() < PATIENT);
    release.send(()).unwrap();
    assert!(disk.flush(PATIENT));
    drop(disk);
    assert_eq!(
        stored_on_disk(&db_in(&dir), &keys, &[work]).stats().entries,
        1
    );
}

#[test]
fn flush_reports_a_writer_that_could_not_open_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let keys = serial_keys(42);
    let disk = DiskListings::open(dir.path(), warm(&ListingCache::default(), &keys, vec![]));
    assert!(!disk.flush(PATIENT));
}
