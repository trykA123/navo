use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use navo_core::ProviderPath;
use navo_fs::FsEntry;
use proptest::prelude::*;

use super::support::entry_sized;
use super::{at, cost, entry, key, listing, names};
use crate::listing::NameLookup;
use crate::{Change, ListingCache};

#[derive(Debug, Clone)]
enum Op {
    Put(u8, u8),
    Get(u8),
    Invalidate(u8),
    Pin(u8),
    Unpin(u8),
    Patch(u8, u8),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (0..6u8, 0..8u8).prop_map(|(k, n)| Op::Put(k, n)),
        2 => (0..6u8).prop_map(Op::Get),
        1 => (0..6u8).prop_map(Op::Invalidate),
        1 => (0..6u8).prop_map(Op::Pin),
        1 => (0..6u8).prop_map(Op::Unpin),
        1 => (0..6u8, 0..12u8).prop_map(|(k, n)| Op::Patch(k, n)),
    ]
}

fn rows(count: u8) -> Vec<String> {
    (0..count).map(|i| format!("row-{i}")).collect()
}

fn listing_of(count: u8) -> crate::CachedListing {
    let names = rows(count);
    listing(&names.iter().map(String::as_str).collect::<Vec<_>>())
}

fn k(index: u8) -> crate::CacheKey {
    key(&format!("k{index}"))
}

fn run(cache: &ListingCache, op: &Op) {
    match op {
        Op::Put(i, n) => cache.put(k(*i), listing_of(*n)),
        Op::Get(i) => drop(cache.get(&k(*i))),
        Op::Invalidate(i) => cache.invalidate(&k(*i)),
        Op::Pin(i) => cache.pin(&k(*i)),
        Op::Unpin(i) => cache.unpin(&k(*i)),
        Op::Patch(i, n) => cache.patch(&k(*i), at(1), [Change::Added(entry(&format!("new-{n}")))]),
    }
}

proptest! {
    #[test]
    fn bytes_stay_in_budget_except_for_pinned_entries(
        budget in 0usize..4000,
        ops in prop::collection::vec(op(), 0..60),
    ) {
        let cache = ListingCache::new(budget);
        for op in &ops {
            run(&cache, op);
            let stats = cache.stats();
            prop_assert!(stats.bytes <= budget || stats.bytes == cache.pinned_bytes());
        }
    }

    #[test]
    fn pinned_byte_counter_matches_a_scan(
        budget in 0usize..4000,
        ops in prop::collection::vec(op(), 0..60),
    ) {
        let cache = ListingCache::new(budget);
        for op in &ops {
            run(&cache, op);
            prop_assert_eq!(cache.pinned_bytes(), cache.scanned_pinned_bytes());
        }
    }

    #[test]
    fn get_returns_the_last_put_until_evicted_or_invalidated(
        budget in 0usize..4000,
        ops in prop::collection::vec(op(), 0..60),
    ) {
        let cache = ListingCache::new(budget);
        let mut last = std::collections::HashMap::new();
        for op in ops.iter().filter(|op| !matches!(op, Op::Patch(..))) {
            run(&cache, op);
            match op {
                Op::Put(i, n) => { last.insert(*i, listing_of(*n)); }
                Op::Invalidate(i) => { last.remove(i); }
                _ => {}
            }
            if let Op::Get(i) | Op::Put(i, _) = op && let Some(found) = cache.get(&k(*i)) {
                prop_assert_eq!(Some(found.as_ref()), last.get(i));
            }
        }
    }

    #[test]
    fn put_that_fits_is_readable_at_once(n in 0u8..8, budget in 0usize..4000) {
        let cache = ListingCache::new(budget);
        cache.put(k(0), listing_of(n));
        let fits = cost(&k(0), &listing_of(n)) <= budget;
        prop_assert_eq!(cache.get(&k(0)).is_some(), fits);
    }

    #[test]
    fn eviction_follows_least_recently_used_order(
        budget in 0usize..3000,
        ops in prop::collection::vec((any::<bool>(), 0..6u8, 0..8u8), 0..60),
    ) {
        let cache = ListingCache::new(budget);
        let mut model: Vec<(u8, usize)> = Vec::new();
        for (is_put, i, n) in ops {
            let found = model.iter().position(|(key, _)| *key == i);
            let touched = found.map(|at| model.remove(at));
            if is_put {
                cache.put(k(i), listing_of(n));
                let size = cost(&k(i), &listing_of(n));
                if size <= budget { model.push((i, size)); }
            } else {
                drop(cache.get(&k(i)));
                model.extend(touched);
            }
            while model.iter().map(|(_, size)| size).sum::<usize>() > budget {
                model.remove(0);
            }
            let bytes: usize = model.iter().map(|(_, size)| size).sum();
            prop_assert_eq!((cache.stats().entries, cache.stats().bytes), (model.len(), bytes));
        }
        let present: BTreeSet<u8> = (0..6).filter(|i| cache.get(&k(*i)).is_some()).collect();
        prop_assert_eq!(present, model.iter().map(|(key, _)| *key).collect::<BTreeSet<u8>>());
    }

    #[test]
    fn patch_keeps_the_order_of_surviving_rows(
        count in 0u8..12,
        edits in prop::collection::vec((0..4u8, 0..16u8, 0..16u8), 0..20),
    ) {
        let cache = ListingCache::new(1 << 20);
        cache.put(k(0), listing_of(count));
        let changes: Vec<Change> = edits.iter().map(|(kind, a, b)| change(*kind, *a, *b)).collect();
        cache.patch(&k(0), at(1), changes);
        let after = names(&cache.get(&k(0)).unwrap_or_else(|| unreachable!()));
        let unique: BTreeSet<&String> = after.iter().collect();
        prop_assert_eq!(unique.len(), after.len());
        let dropped: BTreeSet<String> = edits.iter().filter(|(kind, ..)| matches!(kind, 1 | 2)).map(|(_, a, _)| format!("row-{a}")).collect();
        let originals: Vec<String> = rows(count).into_iter().filter(|name| !dropped.contains(name)).collect();
        let kept: Vec<&String> = after.iter().filter(|name| originals.contains(name)).collect();
        let expected: Vec<&String> = originals.iter().filter(|name| after.contains(name)).collect();
        prop_assert_eq!(&kept, &expected);
        prop_assert_eq!(expected.len(), originals.len());
    }
}

const POOL: [&str; 6] = ["a", "A", "b", "B", "c", "C"];

fn name_pool() -> impl Strategy<Value = Vec<String>> {
    prop::collection::hash_set(prop::sample::select(POOL.to_vec()), 0..7).prop_map(|names| {
        let mut names: Vec<String> = names.into_iter().map(str::to_owned).collect();
        names.sort();
        names
    })
}

fn pick(pool: &[String], index: u8) -> String {
    if pool.is_empty() {
        format!("n{index}")
    } else {
        pool[usize::from(index) % pool.len()].clone()
    }
}

fn edit(kind: u8, a: u8, b: u8, pool: &[String]) -> Change {
    let target = |name: &str| entry_sized(name, u64::from(a));
    match kind {
        0 => Change::Added(target(&format!("new-{a}"))),
        1 => Change::Added(target(&pick(pool, a))),
        2 => Change::Modified(target(&pick(pool, a))),
        3 => Change::Removed(ProviderPath::from_text(&pick(pool, a))),
        4 => Change::Renamed {
            from: ProviderPath::from_text(&pick(pool, a)),
            to: target(&pick(pool, b)),
        },
        5 => Change::Renamed {
            from: ProviderPath::from_text("missing"),
            to: target(&pick(pool, b)),
        },
        6 => {
            let name = pick(pool, a);
            Change::Renamed {
                from: ProviderPath::from_text(&name),
                to: target(&name),
            }
        }
        _ => Change::Renamed {
            from: ProviderPath::from_text(&pick(pool, a)),
            to: target(&format!("new-{b}")),
        },
    }
}

// The pre-plan patch path, kept here as the oracle.
struct IndexedRows {
    rows: Vec<Option<FsEntry>>,
    index: HashMap<ProviderPath, usize>,
}

impl IndexedRows {
    fn new(rows: &[FsEntry]) -> Self {
        let index = rows
            .iter()
            .enumerate()
            .map(|(at, row)| (row.name.clone(), at))
            .collect();
        Self {
            rows: rows.iter().cloned().map(Some).collect(),
            index,
        }
    }

    fn apply(&mut self, change: &Change) {
        match change {
            Change::Added(entry) | Change::Modified(entry) => self.upsert(entry.clone()),
            Change::Removed(name) => self.remove(name),
            Change::Renamed { from, to } => self.rename(from, to.clone()),
        }
    }

    fn upsert(&mut self, entry: FsEntry) {
        match self.index.get(&entry.name) {
            Some(&at) => self.rows[at] = Some(entry),
            None => self.push(entry),
        }
    }

    fn push(&mut self, entry: FsEntry) {
        self.index.insert(entry.name.clone(), self.rows.len());
        self.rows.push(Some(entry));
    }

    fn remove(&mut self, name: &ProviderPath) {
        if let Some(at) = self.index.remove(name) {
            self.rows[at] = None;
        }
    }

    fn rename(&mut self, from: &ProviderPath, to: FsEntry) {
        if *from != to.name {
            self.remove(&to.name);
        }
        match self.index.remove(from) {
            Some(at) => {
                self.index.insert(to.name.clone(), at);
                self.rows[at] = Some(to);
            }
            None => self.push(to),
        }
    }

    fn into_rows(self) -> Arc<[FsEntry]> {
        self.rows.into_iter().flatten().collect()
    }
}

fn oracle_rows(rows: &[FsEntry], changes: &[Change]) -> Arc<[FsEntry]> {
    let mut oracle = IndexedRows::new(rows);
    for change in changes {
        oracle.apply(change);
    }
    oracle.into_rows()
}

proptest! {
    #[test]
    fn patched_rows_and_lookup_match_the_indexed_rows_oracle(
        pool in name_pool(),
        edits in prop::collection::vec((0u8..8, 0u8..8, 0u8..8), 0..30),
    ) {
        let names: Vec<&str> = pool.iter().map(String::as_str).collect();
        let base = listing(&names);
        let changes: Vec<Change> = edits.iter().map(|(kind, a, b)| edit(*kind, *a, *b, &pool)).collect();
        let (first, second) = changes.split_at(changes.len() / 2);
        let (mid, lookup) = base.patched_with(first, at(1), None);
        let (out, lookup) = mid.patched_with(second, at(2), lookup);
        let oracle = oracle_rows(&base.rows, &changes);
        prop_assert_eq!(out.rows.as_ref(), oracle.as_ref());
        if let Some(lookup) = lookup {
            for (at, row) in out.rows.iter().enumerate() {
                prop_assert_eq!(lookup.find(&out.rows, &row.name), Some(at as u32));
            }
        }
    }

    #[test]
    fn a_colliding_lookup_matches_the_indexed_rows_oracle(
        pool in name_pool(),
        edits in prop::collection::vec((0u8..8, 0u8..8, 0u8..8), 0..30),
    ) {
        let names: Vec<&str> = pool.iter().map(String::as_str).collect();
        let base = listing(&names);
        let lookup = NameLookup::colliding(&base.rows);
        let changes: Vec<Change> = edits.iter().map(|(kind, a, b)| edit(*kind, *a, *b, &pool)).collect();
        let (out, _) = base.patched_with(&changes, at(1), lookup);
        let oracle = oracle_rows(&base.rows, &changes);
        prop_assert_eq!(out.rows.as_ref(), oracle.as_ref());
    }
}

#[test]
fn two_patches_reuse_the_lookup_seed_and_bound_the_map() {
    let names: Vec<String> = (0..1_000).map(|index| format!("row-{index:04}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing(&refs));
    cache.patch(
        &key("a"),
        at(1),
        [Change::Modified(entry_sized("row-0000", 1))],
    );
    let seed = {
        let store = cache.lock();
        let lookup = store
            .lookup(&key("a"))
            .expect("the first patch builds a lookup");
        lookup.key_of(&ProviderPath::from_text("row-0000"))
    };
    cache.patch(
        &key("a"),
        at(2),
        [Change::Modified(entry_sized("row-0500", 2))],
    );
    let store = cache.lock();
    let lookup = store
        .lookup(&key("a"))
        .expect("the second patch keeps the lookup");
    assert_eq!(lookup.key_of(&ProviderPath::from_text("row-0000")), seed);
    assert!(
        lookup.capacity() >= refs.len(),
        "capacity {} below {} rows",
        lookup.capacity(),
        refs.len()
    );
    assert!(
        lookup.capacity() <= 2 * refs.len(),
        "capacity {} over twice {} rows",
        lookup.capacity(),
        refs.len()
    );
}

#[test]
fn put_and_invalidate_drop_the_lookup() {
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing(&["a", "b"]));
    cache.patch(&key("a"), at(1), [Change::Modified(entry("a"))]);
    assert!(cache.lock().lookup(&key("a")).is_some());
    let mut replacement = listing(&["c"]);
    replacement.read_at = at(2);
    cache.put(key("a"), replacement);
    assert!(cache.lock().lookup(&key("a")).is_none());
    cache.patch(&key("a"), at(3), [Change::Modified(entry("c"))]);
    assert!(cache.lock().lookup(&key("a")).is_some());
    cache.invalidate(&key("a"));
    assert!(cache.lock().lookup(&key("a")).is_none());
}

#[test]
fn a_forced_collision_mid_batch_matches_the_indexed_rows() {
    let base = listing(&["a"]);
    let lookup = NameLookup::colliding(&base.rows).expect("one row has one hash");
    let changes = [
        Change::Modified(entry_sized("a", 7)),
        Change::Added(entry_sized("b", 8)),
    ];
    let (out, lookup) = base.patched_with(&changes, at(1), Some(lookup));
    let oracle = oracle_rows(&base.rows, &changes);
    assert_eq!(out.rows.as_ref(), oracle.as_ref());
    assert!(lookup.is_none(), "the collided batch drops the lookup");
}

#[test]
fn a_rename_to_a_fresh_name_keeps_the_row_and_the_lookup() {
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing(&["a", "b", "c"]));
    cache.patch(
        &key("a"),
        at(1),
        [Change::Renamed {
            from: ProviderPath::from_text("b"),
            to: entry("z"),
        }],
    );
    cache.patch(&key("a"), at(2), [Change::Modified(entry("z"))]);
    let after = cache.get(&key("a")).expect("the listing is cached");
    assert_eq!(names(&after), ["a", "z", "c"]);
    assert_eq!(
        after
            .rows
            .iter()
            .filter(|row| row.name == ProviderPath::from_text("z"))
            .count(),
        1
    );
    let store = cache.lock();
    let lookup = store
        .lookup(&key("a"))
        .expect("both patches keep the lookup");
    assert_eq!(
        lookup.find(&after.rows, &ProviderPath::from_text("z")),
        Some(1)
    );
}

#[test]
fn a_batch_that_reuses_a_vacated_name_keeps_the_lookup() {
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing(&["a", "c"]));
    cache.patch(
        &key("a"),
        at(1),
        [
            Change::Renamed {
                from: ProviderPath::from_text("a"),
                to: entry("b"),
            },
            Change::Modified(entry("a")),
        ],
    );
    let after = cache.get(&key("a")).expect("the listing is cached");
    assert_eq!(names(&after), ["b", "c", "a"]);
    let store = cache.lock();
    let lookup = store
        .lookup(&key("a"))
        .expect("the rebuild keeps the lookup");
    assert_eq!(
        lookup.find(&after.rows, &ProviderPath::from_text("b")),
        Some(0)
    );
    assert_eq!(
        lookup.find(&after.rows, &ProviderPath::from_text("c")),
        Some(1)
    );
    assert_eq!(
        lookup.find(&after.rows, &ProviderPath::from_text("a")),
        Some(2)
    );
}

#[test]
fn a_removal_pass_shrinks_the_lookup_map() {
    let names: Vec<String> = (0..100_000)
        .map(|index| format!("row-{index:06}"))
        .collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let base = listing(&refs);
    let lookup = NameLookup::build(&base.rows).expect("unique names build a lookup");
    let changes: Vec<Change> = names[10..]
        .iter()
        .map(|name| Change::Removed(ProviderPath::from_text(name)))
        .collect();
    let (out, lookup) = base.patched_with(&changes, at(1), Some(lookup));
    let lookup = lookup.expect("a removal pass keeps the lookup");
    assert_eq!(out.rows.len(), 10);
    assert!(
        lookup.bytes() <= 2 * out.rows.len() * 13 + size_of::<NameLookup>(),
        "lookup bytes {} for {} rows",
        lookup.bytes(),
        out.rows.len()
    );
    for (at, row) in out.rows.iter().enumerate() {
        assert_eq!(lookup.find(&out.rows, &row.name), Some(at as u32));
    }
}

#[test]
fn an_empty_patch_keeps_the_rows_and_the_lookup() {
    let cache = ListingCache::new(1 << 20);
    cache.put(key("a"), listing(&["a", "b"]));
    cache.patch(&key("a"), at(1), [Change::Modified(entry("a"))]);
    let before = cache.get(&key("a")).expect("the listing is cached");
    let seed = {
        let store = cache.lock();
        store
            .lookup(&key("a"))
            .expect("the first patch builds a lookup")
            .key_of(&ProviderPath::from_text("a"))
    };
    cache.patch(&key("a"), at(2), []);
    let after = cache.get(&key("a")).expect("the listing is still cached");
    assert!(Arc::ptr_eq(&before.rows, &after.rows));
    assert_eq!(after.read_at, before.read_at);
    let store = cache.lock();
    let lookup = store
        .lookup(&key("a"))
        .expect("the empty patch keeps the lookup");
    assert_eq!(lookup.key_of(&ProviderPath::from_text("a")), seed);
}

fn change(kind: u8, a: u8, b: u8) -> Change {
    let name = |i: u8| format!("row-{i}");
    match kind {
        0 => Change::Added(entry(&name(a))),
        1 => Change::Removed(ProviderPath::from_text(&name(a))),
        2 => Change::Renamed {
            from: ProviderPath::from_text(&name(a)),
            to: entry(&format!("moved-{b}")),
        },
        _ => Change::Modified(entry(&name(a))),
    }
}
