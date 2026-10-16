use std::collections::BTreeSet;

use proptest::prelude::*;
use rusty_core::ProviderPath;

use super::{cost, entry, key, listing, names};
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
        Op::Patch(i, n) => cache.patch(&k(*i), [Change::Added(entry(&format!("new-{n}")))]),
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
        cache.patch(&k(0), changes);
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
