//! Shared deep-build lifecycle measurements; scale assertions belong to S8.

use super::{Measured, NtfsIndex, Probe, SizeHint, held, hint, save};
use crate::ntfs::Enumeration;
use crate::ntfs::test_support::DeepMft;
use crate::ntfs::volume_io::{VolumeIo, enumerate_volume_sized, read_volume_journal};
use crate::pipe::CancelToken;
use crate::search::{Term, search};

/// Owns the probe and real generations across checkpoint, conversion, build and promotion.
pub(super) struct PeakHarness {
    pub(super) probe: Probe,
    pub(super) old: Option<NtfsIndex>,
    pub(super) old_bytes: usize,
    hint: SizeHint,
    path: std::path::PathBuf,
    _folder: tempfile::TempDir,
}

impl PeakHarness {
    /// Measures a real old deep index, or starts empty; `files` counts old files, not folders.
    /// Panics on fixture errors or unaccounted allocations.
    pub(super) fn new(files: Option<u32>) -> Self {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("peak.index");
        let probe = Probe::start();
        let old = files.map(|files| DeepMft::ordered(files).read().index);
        let hint = old.as_ref().map_or_else(SizeHint::default, hint);
        let old_bytes = probe.mark();
        assert_eq!(
            old_bytes,
            old.as_ref().map_or(0, accounted_bytes),
            "peak harness must account for every old allocation byte"
        );
        Self {
            probe,
            old,
            old_bytes,
            hint,
            path,
            _folder: folder,
        }
    }

    /// Checkpoints/converts old storage, builds through production I/O, queries and replays.
    /// Panics on I/O errors, unresolved changes or lost search results.
    pub(super) fn build(&mut self, volume: &mut impl VolumeIo) -> Enumeration {
        if let Some(old) = &mut self.old {
            save(&self.path, old);
            old.make_search_only();
            query_name(old);
        }
        let mut candidate =
            enumerate_volume_sized(volume, 'C', usize::MAX, self.hint, &CancelToken::new())
                .unwrap();
        if let Some(old) = &self.old {
            query_name(old);
        }
        query_name(&candidate.index);
        assert!(candidate.unresolved.is_empty());
        let batch = read_volume_journal(volume, candidate.index.position).unwrap();
        assert!(candidate.index.apply_batch(batch).unwrap().is_empty());
        assert!(candidate.index.snapshot().is_some());
        candidate
    }

    /// Streams v3 save, drops old storage, trims, and measures final bytes by dropping the index.
    /// Panics if saving fails or the final index allocations are not fully accounted.
    pub(super) fn finish(mut self, mut index: NtfsIndex) -> (Measured, usize) {
        save(&self.path, &index);
        drop(self.old.take());
        index.table.trim_slack();
        let accounted = accounted_bytes(&index);
        let measured = self.probe.read();
        let retained = held(index, &self.probe);
        assert_eq!(retained, accounted);
        (measured, retained)
    }
}

/// Counts owned table, identity and pending-rename allocations, in bytes.
fn accounted_bytes(index: &NtfsIndex) -> usize {
    index.table.memory_bytes() + index.ids.memory_bytes() + index.pending_renames().memory_bytes()
}

/// Executes a real name search and checks its nested path; panics on a broken deep fixture.
fn query_name(index: &NtfsIndex) {
    let hits = search(&index.table, &[Term::Name("entry-0000000.txt")], 1, &|| {
        false
    });
    assert_eq!(hits.len(), 1);
    let path = index.table.path_with(hits[0].id, '\\').unwrap();
    assert_eq!(path.matches('\\').count(), 5);
}

#[test]
fn peak_harness_accounts_for_real_old_drop_and_complete_lifecycle() {
    {
        let mut harness = PeakHarness::new(Some(1_200));
        let old = harness.old.take().unwrap();
        assert_eq!(harness.old_bytes, held(old, &harness.probe));
    }
    for old_files in [None, Some(1_200)] {
        let mut harness = PeakHarness::new(old_files);
        let old_bytes = harness.old_bytes;
        let candidate = harness.build(&mut DeepMft::new(1_200));
        assert_eq!(candidate.index.table.len(), 1_321);
        assert_eq!(candidate.index.table.folder_count(), 121);
        if let Some(old) = &harness.old {
            assert!(old.is_search_only());
            assert!(old.snapshot().is_none());
        }
        let accounted = accounted_bytes(&candidate.index);
        let (measured, final_bytes) = harness.finish(candidate.index);
        assert!(measured.peak >= old_bytes.max(final_bytes));
        assert!(final_bytes <= accounted);
        assert!(final_bytes > 0);
    }
}
