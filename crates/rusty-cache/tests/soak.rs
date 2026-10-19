#![allow(
    clippy::print_stderr,
    reason = "the soak prints its numbers for the CI log"
)]
#![allow(
    clippy::expect_used,
    reason = "the soak fails on a broken fake file system"
)]

use std::time::{Instant, SystemTime};

use rusty_cache::{CacheKey, CacheKeys, CachedListing, Change, ListingCache};
use rusty_core::Location;
use rusty_fs::{CancelToken, FakeFs, load_folder};

const GROUPS: usize = 200;
const LEAVES: usize = 250;
const FOLDERS: usize = GROUPS * LEAVES;
const TABS: usize = 10_000;
const WARM_UP_ROUNDS: usize = 2;
const OPEN_AT_ONCE: usize = 16;
const MOVES: usize = 6;
const BUDGET: usize = 8 * 1024 * 1024;
const HEAP_SLACK: f64 = 0.02;

fn group_path(group: usize) -> String {
    format!("/g{group:03}")
}

fn leaf_path(leaf: usize) -> String {
    format!("{}/f{leaf:05}", group_path(leaf / LEAVES))
}

fn build_fs() -> FakeFs {
    let fs = FakeFs::new();
    for leaf in 0..FOLDERS {
        let path = leaf_path(leaf);
        fs.add_folder(&path);
        for file in 0..leaf % 13 {
            fs.add_file(&format!("{path}/file-{file:02}.dat"), file as u64);
        }
    }
    fs
}

fn every_folder(fs: &FakeFs) -> Vec<Location> {
    let groups = (0..GROUPS).map(group_path);
    let leaves = (0..FOLDERS).map(leaf_path);
    std::iter::once("/".to_owned())
        .chain(groups)
        .chain(leaves)
        .map(|path| fs.location(&path))
        .collect()
}

fn read(fs: &FakeFs, folder: &Location) -> CachedListing {
    let snapshot = load_folder(fs, folder, &CancelToken::new()).expect("the fake folder lists");
    CachedListing {
        rows: snapshot.details.into(),
        read_at: SystemTime::now(),
        folder_changed: None,
        complete: true,
    }
}

fn cost(key: &CacheKey, listing: &CachedListing) -> usize {
    let probe = ListingCache::new(usize::MAX);
    probe.put(key.clone(), listing.clone());
    probe.stats().bytes
}

struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % bound as u64) as usize
    }
}

#[derive(Debug, Default)]
struct Peak {
    unpinned: usize,
    pinned: usize,
    pinned_entries: usize,
    total: usize,
    reads: usize,
    checks: usize,
}

struct Tab {
    folder: Location,
    pins: Vec<CacheKey>,
}

struct Soak<'a> {
    fs: &'a FakeFs,
    cache: ListingCache,
    keys: CacheKeys,
    tabs: Vec<Tab>,
    rng: Rng,
    largest: usize,
    peak: Peak,
}

impl<'a> Soak<'a> {
    fn new(fs: &'a FakeFs, largest: usize) -> Self {
        Self {
            fs,
            cache: ListingCache::new(BUDGET),
            keys: CacheKeys::default(),
            tabs: Vec::new(),
            rng: Rng(0x9e37_79b9_7f4a_7c15),
            largest,
            peak: Peak::default(),
        }
    }

    fn key(&self, folder: &Location) -> CacheKey {
        self.keys
            .key(folder)
            .expect("fake locations always have a key")
    }

    fn random_leaf(&mut self) -> Location {
        let leaf = self.rng.below(FOLDERS);
        self.fs.location(&leaf_path(leaf))
    }

    fn show(&mut self, folder: &Location) {
        let key = self.key(folder);
        if self.cache.get(&key).is_none() {
            let listing = read(self.fs, folder);
            self.cache.put(key, listing);
            self.peak.reads += 1;
        }
    }

    fn navigate(&mut self, tab: usize, folder: Location) {
        let pins: Vec<CacheKey> = std::iter::once(folder.clone())
            .chain(folder.parent())
            .map(|location| self.key(&location))
            .collect();
        pins.iter().for_each(|key| self.cache.pin(key));
        let old = std::mem::replace(&mut self.tabs[tab].pins, pins);
        old.iter().for_each(|key| self.cache.unpin(key));
        self.tabs[tab].folder = folder.clone();
        self.show(&folder);
        self.check();
    }

    fn open_tab(&mut self) {
        let folder = self.random_leaf();
        self.tabs.push(Tab {
            folder: folder.clone(),
            pins: Vec::new(),
        });
        self.navigate(self.tabs.len() - 1, folder);
    }

    fn close_tab(&mut self, tab: usize) {
        let closed = self.tabs.swap_remove(tab);
        closed.pins.iter().for_each(|key| self.cache.unpin(key));
        self.check();
    }

    fn move_in(&mut self, tab: usize) {
        let here = self.tabs[tab].folder.clone();
        match self.rng.below(6) {
            0 => {
                let up = here.parent().unwrap_or(here);
                self.navigate(tab, up);
            }
            1 => self.prefetch_sibling(),
            2 => self.patch(&here),
            3 => self.cache.invalidate(&self.key(&here)),
            _ => {
                let next = self.random_leaf();
                self.navigate(tab, next);
            }
        }
        self.check();
    }

    fn prefetch_sibling(&mut self) {
        let folder = self.random_leaf();
        self.show(&folder);
    }

    fn patch(&mut self, folder: &Location) {
        let key = self.key(folder);
        let Some(listing) = self.cache.get(&key) else {
            return;
        };
        let Some(first) = listing.rows.first() else {
            return;
        };
        let change = Change::Modified(first.clone());
        self.cache.patch(&key, SystemTime::now(), [change]);
    }

    fn check(&mut self) {
        let stats = self.cache.stats();
        let unpinned = stats.bytes - stats.pinned_bytes;
        let open = self.tabs.len();
        assert!(unpinned <= BUDGET, "unpinned {unpinned} over {BUDGET}");
        assert!(
            stats.pinned_entries <= 2 * open,
            "{} pinned listings for {open} tabs",
            stats.pinned_entries
        );
        assert!(
            stats.pinned_bytes <= 2 * open * self.largest,
            "pinned {} for {open} tabs",
            stats.pinned_bytes
        );
        let peak = &mut self.peak;
        peak.unpinned = peak.unpinned.max(unpinned);
        peak.pinned = peak.pinned.max(stats.pinned_bytes);
        peak.pinned_entries = peak.pinned_entries.max(stats.pinned_entries);
        peak.total = peak.total.max(stats.bytes);
        peak.checks += 1;
    }

    fn churn(&mut self, tabs: usize) {
        for _ in 0..tabs {
            if self.tabs.len() == OPEN_AT_ONCE {
                let tab = self.rng.below(OPEN_AT_ONCE);
                self.close_tab(tab);
            }
            self.open_tab();
            let tab = self.tabs.len() - 1;
            for _ in 0..MOVES {
                self.move_in(tab);
            }
        }
        while !self.tabs.is_empty() {
            self.close_tab(self.tabs.len() - 1);
        }
    }
}

fn largest_listing(fs: &FakeFs, folders: &[Location]) -> usize {
    let keys = CacheKeys::default();
    folders
        .iter()
        .map(|folder| cost(&keys.key(folder).expect("a key"), &read(fs, folder)))
        .max()
        .unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn private_bytes() -> Option<u64> {
    use rusty_diag::MemoryProbe;
    Some(rusty_diag::ProcStatusProbe.sample().ok()?.private_bytes)
}

#[cfg(not(target_os = "linux"))]
fn private_bytes() -> Option<u64> {
    None
}

fn run_soak(fs: &FakeFs, largest: usize, tabs: usize) -> Peak {
    let mut soak = Soak::new(fs, largest);
    soak.churn(tabs);
    let stats = soak.cache.stats();
    assert_eq!((stats.pinned_bytes, stats.pinned_entries), (0, 0));
    assert!(stats.bytes <= BUDGET, "{} left after the pins", stats.bytes);
    eprintln!(
        "soak: after the last tab closed {} bytes in {} listings, {} evictions",
        stats.bytes, stats.entries, stats.evictions
    );
    soak.peak
}

#[test]
fn ten_thousand_tabs_over_fifty_thousand_folders_stay_in_budget_and_give_the_heap_back() {
    let started = Instant::now();
    let fs = build_fs();
    let folders = every_folder(&fs);
    let largest = largest_listing(&fs, &folders);
    drop(folders);
    let cold = private_bytes();
    for _ in 0..WARM_UP_ROUNDS {
        run_soak(&fs, largest, TABS);
    }
    let baseline = private_bytes();
    let peak = run_soak(&fs, largest, TABS);
    let after = private_bytes();
    eprintln!(
        "soak: {TABS} tabs, {FOLDERS} folders, budget {BUDGET}, peak unpinned {}, peak pinned {} in {} listings, peak total {}, {} reads, {} checks, {:.1} s",
        peak.unpinned,
        peak.pinned,
        peak.pinned_entries,
        peak.total,
        peak.reads,
        peak.checks,
        started.elapsed().as_secs_f64()
    );
    eprintln!("soak: private heap cold {cold:?}, baseline {baseline:?}, after {after:?}");
    if let (Some(baseline), Some(after)) = (baseline, after) {
        let limit = baseline as f64 * (1.0 + HEAP_SLACK);
        assert!(
            after as f64 <= limit,
            "heap {after} is over {limit:.0} (baseline {baseline})"
        );
    }
}
