use std::time::Instant;

use super::*;
use crate::{EntryBatch, FakeFs};

#[test]
fn listing_waits_for_configured_delay() {
    let fake = Arc::new(FakeFs::new());
    fake.add_file("folder/file", 42);
    let folder = fake.location("folder");
    let delay = Duration::from_millis(30);
    let slow = SlowFs::new(fake, delay);
    let mut entries = Vec::new();
    let started = Instant::now();

    slow.list(
        &folder,
        &mut |batch: EntryBatch| {
            entries.extend(batch.entries);
            ControlFlow::Continue(())
        },
        &CancelToken::new(),
    )
    .unwrap();

    assert!(started.elapsed() >= delay);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name.to_text(), "file");
    assert_eq!(entries[0].size, 42);
}

#[test]
fn pure_queries_delegate_unchanged() {
    let fake = Arc::new(FakeFs::new());
    let slow = SlowFs::new(fake.clone(), Duration::from_millis(30));

    assert_eq!(slow.id(), fake.id());
    assert_eq!(slow.capabilities(), fake.capabilities());
    assert_eq!(slow.supports_atomic_rename(), fake.supports_atomic_rename());
}

#[test]
fn invalid_or_zero_delay_disables_simulation() {
    for value in [
        None,
        Some("0"),
        Some("garbage"),
        Some(""),
        Some("-1"),
        Some("18446744073709551616"),
    ] {
        assert_eq!(SlowFs::parse_delay_ms(value), None, "{value:?}");
    }
}

#[test]
fn positive_delay_enables_simulation() {
    assert_eq!(
        SlowFs::parse_delay_ms(Some("30")),
        Some(Duration::from_millis(30))
    );
}
