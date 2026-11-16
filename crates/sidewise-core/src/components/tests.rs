use std::sync::{Arc, Mutex};

use super::*;

fn used(bytes: u64, items: u64) -> Reading {
    Reading::Use(MemoryUse { bytes, items })
}

#[test]
fn registered_reporters_are_sampled_largest_first() {
    let registry = ComponentRegistry::new();
    registry.register("small", || used(10, 1));
    registry.register("large", || used(5_000, 20));
    let usage = registry.usage();
    let names: Vec<&str> = usage.iter().map(|component| component.name).collect();
    assert_eq!(names, ["large", "small"]);
    assert_eq!(usage[0].items, 20);
    assert!(!usage[0].busy);
}

#[test]
fn a_reporter_whose_lock_is_held_reports_busy_without_blocking() {
    let registry = ComponentRegistry::new();
    let state = Arc::new(Mutex::new(42_u64));
    let shared = Arc::clone(&state);
    registry.register("locked", move || match shared.try_lock() {
        Ok(bytes) => used(*bytes, 1),
        Err(_) => Reading::Busy,
    });
    let guard = state.lock().unwrap();
    let usage = registry.usage();
    drop(guard);
    assert_eq!(
        usage,
        [ComponentUse {
            name: "locked",
            bytes: 0,
            items: 0,
            busy: true
        }]
    );
    assert_eq!(registry.usage()[0].bytes, 42);
}

#[test]
fn reporters_with_one_name_add_up() {
    let registry = ComponentRegistry::new();
    registry.register("preview body", || used(100, 1));
    registry.register("preview body", || used(50, 1));
    registry.register("preview body", || Reading::Busy);
    let usage = registry.usage();
    assert_eq!(usage.len(), 1);
    assert_eq!(
        (usage[0].bytes, usage[0].items, usage[0].busy),
        (150, 2, true)
    );
}

#[test]
fn a_gone_reporter_is_dropped_from_the_registry() {
    let registry = ComponentRegistry::new();
    let owner = Arc::new(());
    let weak = Arc::downgrade(&owner);
    registry.register("owned", move || match weak.upgrade() {
        Some(_) => used(1, 1),
        None => Reading::Gone,
    });
    assert_eq!(registry.usage().len(), 1);
    drop(owner);
    assert!(registry.usage().is_empty());
    assert_eq!(registry.reporters.lock().unwrap().len(), 0);
}

static TEST_TALLY: Tally = Tally::new("test tally");

#[test]
fn a_tally_counts_held_bytes_until_they_drop() {
    let mut first = TEST_TALLY.hold(MemoryUse {
        bytes: 1_000,
        items: 1,
    });
    let second = TEST_TALLY.hold(MemoryUse {
        bytes: 24,
        items: 1,
    });
    assert_eq!(
        TEST_TALLY.read(),
        MemoryUse {
            bytes: 1_024,
            items: 2
        }
    );
    first.set(MemoryUse {
        bytes: 10,
        items: 3,
    });
    assert_eq!(
        TEST_TALLY.read(),
        MemoryUse {
            bytes: 34,
            items: 4
        }
    );
    drop(first);
    drop(second);
    assert_eq!(TEST_TALLY.read(), MemoryUse::default());
    assert!(
        usage()
            .iter()
            .any(|component| component.name == "test tally")
    );
}
