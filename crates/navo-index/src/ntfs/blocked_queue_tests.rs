//! Retained storage guard for the out-of-order journal queue.

use super::super::{JournalPosition, NtfsIndex, Pending};
use crate::table::{Flags, TableChange};

/// Measures queue bytes beyond 300,000 names before any parent arrives.
fn blocked_record_bytes() -> usize {
    let pos = JournalPosition {
        journal_id: 7,
        next_usn: 100,
    };
    let mut index = NtfsIndex::new('C', pos).unwrap();
    let probe = crate::heap_probe::Probe::start();
    let (mut pending, mut names) = (Pending::default(), 0);
    for number in 0..300_000 {
        let (file_id, name) = (number as u128 + 100, format!("entry-{number:07}.txt"));
        names += name.len();
        let add = TableChange::Add {
            file_id,
            parent_id: 10_000_000,
            name,
            flags: Flags::NONE,
        };
        pending.push(&mut index, add).unwrap();
        let size = TableChange::Size { file_id, bytes: 1 };
        pending.push(&mut index, size).unwrap();
    }
    (probe.read().retained as usize - names).div_ceil(300_000)
}

#[test]
fn blocked_queue_storage_stays_within_48_bytes() {
    let overhead = blocked_record_bytes();
    assert!(overhead <= 48, "blocked {overhead} B/record > 48");
}
