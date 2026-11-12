//! MFT build peaks with late, nested parents, measured through the volume adapter.
#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    reason = "memory fixtures assert their layout and print measured budgets"
)]

use super::peak_tests::held;
use super::test_support::MftFixture;
use super::volume_io::{VolumeIo, enumerate_volume};
use super::{JournalPosition, NtfsError};
use crate::heap_probe::Probe;
use crate::pipe::CancelToken;

#[path = "blocked_queue_tests.rs"]
mod blocked_queue_tests;

/// Emits 30% of files before their parents, followed by three-level folder chains from leaves up.
struct BlockedMft {
    inner: MftFixture,
    cursor: u64,
    files: u32,
    blocked: u32,
    groups: u32,
}

impl BlockedMft {
    /// Creates exactly `entries` entries including the root, with 1% folders in chains of three.
    fn new(entries: u32) -> Self {
        let groups = entries / 300;
        let files = entries - 1 - groups * 3;
        Self {
            inner: MftFixture::new(entries, 7),
            cursor: 0,
            files,
            blocked: files * 3 / 10,
            groups,
        }
    }

    /// Returns a record's parent reference and NTFS attributes for the late-parent layout.
    fn parent(&self, number: u32) -> (u64, u32) {
        if number <= self.blocked {
            let leaf = self.files + 1 + (number - 1) % self.groups * 3;
            return (u64::from(leaf) + 100, 0);
        }
        if number <= self.files {
            return (5, 0);
        }
        let level = (number - self.files - 1) % 3;
        let parent = if level == 2 {
            5
        } else {
            u64::from(number) + 101
        };
        (parent, 0x10)
    }
}

impl VolumeIo for BlockedMft {
    /// Supplies the same journal metadata as the ordinary streaming MFT fixture.
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        self.inner.query_journal()
    }

    /// Rewrites each bounded MFT buffer to put children before their missing folder chain.
    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        if cursor == 0 {
            self.inner = MftFixture::new(self.files + 1 + self.groups * 3, 7);
        } else {
            assert_eq!(cursor, self.cursor);
        }
        let Some(mut bytes) = self.inner.enumerate(cursor)? else {
            return Ok(None);
        };
        self.cursor = u64::from_le_bytes(bytes[..8].try_into().unwrap());
        let mut offset = 8;
        while offset < bytes.len() {
            let length = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
            let reference = u64::from_le_bytes(bytes[offset + 8..offset + 16].try_into().unwrap());
            let (parent, attributes) = self.parent(u32::try_from(reference - 100).unwrap());
            bytes[offset + 16..offset + 24].copy_from_slice(&parent.to_le_bytes());
            bytes[offset + 52..offset + 56].copy_from_slice(&attributes.to_le_bytes());
            offset += length;
        }
        Ok(Some(bytes))
    }

    /// Enumeration never reads a journal batch.
    fn read_journal(&mut self, position: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        self.inner.read_journal(position)
    }

    /// Each synthetic file contains one byte.
    fn file_size(&mut self, file_id: u128) -> Result<u64, NtfsError> {
        self.inner.file_size(file_id)
    }
}

/// Reports total build peak and retained bytes for a blocked-parent MFT.
fn build_blocked(entries: u32) {
    let mut fixture = BlockedMft::new(entries);
    let probe = Probe::start();
    let started = std::time::Instant::now();
    let result = enumerate_volume(&mut fixture, 'C', usize::MAX, &CancelToken::new()).unwrap();
    let elapsed = started.elapsed();
    let measured = probe.read();
    assert!(result.unresolved.is_empty());
    assert_eq!(result.index.table.len(), entries as usize);
    assert_eq!(
        result.index.table.folder_count(),
        fixture.groups as usize * 3 + 1
    );
    let child = result.index.ids[&101];
    assert_eq!(result.index.table.size(child), Some(1));
    let path = result.index.table.path_with(child, '\\').unwrap();
    assert_eq!(path.matches('\\').count(), 4);
    let retained = held(result.index, &probe);
    println!(
        "RAM build_blocked: peak {} KiB, retained {} KiB",
        measured.peak >> 10,
        retained >> 10,
    );
    println!(
        "BLOCKED {entries}: ratio {:.3}, build {} ms",
        measured.peak as f64 / retained as f64,
        elapsed.as_millis(),
    );
}

#[test]
fn build_blocked_stays_within_its_peak() {
    build_blocked(1_000_000);
}

#[test]
#[ignore = "scale report"]
fn blocked_scale_report() {
    for entries in [2_000_000, 5_000_000] {
        build_blocked(entries);
    }
}
