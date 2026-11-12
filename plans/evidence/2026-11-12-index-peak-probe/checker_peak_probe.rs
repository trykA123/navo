//! Throwaway checker measurements for deep, out-of-order MFT builds; never ship this module.

use super::*;
use crate::ntfs::volume_io::VolumeIo;
use crate::ntfs::{Enumeration, JournalPosition, NtfsError, Pending};
use crate::table::TableChange;
use crate::usn::parse_batch;
use std::time::Instant;

struct DeepMft {
    files: u32,
    next: u32,
    ordered: bool,
}

impl DeepMft {
    /// Streams files plus ten percent directories in four-deep independent trees.
    fn new(files: u32, ordered: bool) -> Self {
        Self {
            files,
            next: 0,
            ordered,
        }
    }

    /// Returns a numeric record and its parent; thirty percent of files precede delayed trees.
    fn item(&self, at: u32) -> (u32, u32, bool) {
        let folders = self.files / 10;
        let delayed_files = self.files * 3 / 10;
        let delayed_folders = folders * 3 / 10;
        let (number, folder) = if self.ordered {
            if at < folders {
                (self.files + at, true)
            } else {
                (at - folders, false)
            }
        } else if at < delayed_files {
            (at, false)
        } else if at < delayed_files + folders - delayed_folders {
            (self.files + delayed_folders + at - delayed_files, true)
        } else if at < self.files + folders - delayed_folders {
            (
                delayed_files + at - (delayed_files + folders - delayed_folders),
                false,
            )
        } else {
            (
                self.files + at - (self.files + folders - delayed_folders),
                true,
            )
        };
        let parent = if folder {
            if (number - self.files).is_multiple_of(4) {
                5
            } else {
                number + 99
            }
        } else {
            self.files + number / 40 * 4 + 3 + 100
        };
        (number + 100, parent, folder)
    }
}

impl VolumeIo for DeepMft {
    /// Supplies a stable journal for the synthetic volume.
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        let mut bytes = vec![0; 56];
        bytes[..8].copy_from_slice(&7u64.to_le_bytes());
        bytes[16..24].copy_from_slice(&100i64.to_le_bytes());
        Ok(bytes)
    }

    /// Encodes at most 512 records, keeping the production 64 KiB buffer limit.
    fn enumerate(&mut self, _: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        let end = (self.next + 512).min(self.files + self.files / 10);
        if end == self.next {
            return Ok(None);
        }
        let mut bytes = u64::from(end).to_le_bytes().to_vec();
        for at in self.next..end {
            let (number, parent, folder) = self.item(at);
            let name = format!("entry-{number:07}.txt");
            let units: Vec<_> = name.encode_utf16().collect();
            let length = (60 + units.len() * 2).next_multiple_of(8);
            let mut wire = vec![0; length];
            wire[..4].copy_from_slice(&(length as u32).to_le_bytes());
            wire[4..6].copy_from_slice(&2u16.to_le_bytes());
            wire[8..16].copy_from_slice(&u64::from(number).to_le_bytes());
            wire[16..24].copy_from_slice(&u64::from(parent).to_le_bytes());
            wire[24..32].copy_from_slice(&90i64.to_le_bytes());
            wire[52..56].copy_from_slice(&(if folder { 0x10u32 } else { 0 }).to_le_bytes());
            wire[56..58].copy_from_slice(&((units.len() * 2) as u16).to_le_bytes());
            wire[58..60].copy_from_slice(&60u16.to_le_bytes());
            for (pair, unit) in wire[60..].chunks_mut(2).zip(units) {
                pair.copy_from_slice(&unit.to_le_bytes());
            }
            bytes.extend(wire);
        }
        assert!(bytes.len() <= 64 * 1024);
        self.next = end;
        Ok(Some(bytes))
    }

    /// Returns an empty journal batch; the measurement never follows changes.
    fn read_journal(&mut self, _: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        Ok(100i64.to_le_bytes().to_vec())
    }

    /// Gives every file one byte, matching the existing peak fixture.
    fn file_size(&mut self, _: u128) -> Result<u64, NtfsError> {
        Ok(1)
    }
}

#[derive(Default, Debug)]
struct Parts {
    peak: usize,
    live: usize,
    columns: [usize; 10],
    logical_columns: usize,
    logical_names: usize,
    frn: usize,
    queue: usize,
    waiters: usize,
    pending_count: usize,
}

/// Estimates hash allocation bytes including control bytes, as FrnMap's accounting does.
fn hash_bytes<T>(capacity: usize) -> usize {
    if capacity == 0 {
        return 0;
    }
    let buckets = if capacity < 8 {
        if capacity < 4 { 4 } else { 8 }
    } else {
        (capacity * 8 / 7).next_power_of_two()
    };
    buckets * (size_of::<T>() + 1) + 16
}

/// Captures capacities at a batch boundary; the unassigned residual covers scratch and copies.
fn parts(index: &NtfsIndex, pending: &Pending, probe: &Probe) -> Parts {
    let reading = probe.read();
    let mut columns = [0; 10];
    for (slot, (_, bytes)) in columns.iter_mut().zip(index.table.column_bytes()) {
        *slot = bytes;
    }
    let waiters = hash_bytes::<(u128, Vec<u128>)>(pending.waiters.capacity())
        + pending
            .waiters
            .values()
            .map(|ids| ids.capacity() * size_of::<u128>())
            .sum::<usize>();
    if let Some(blocked) = pending.blocked.values().next() {
        assert_eq!(blocked.changes.capacity(), 4);
        assert_eq!(blocked.changes.len(), 2);
    }
    let queue = hash_bytes::<(u128, crate::ntfs::Blocked)>(pending.blocked.capacity())
        + pending.blocked.len() * (4 * size_of::<TableChange>() + 17);
    Parts {
        peak: reading.peak,
        live: reading.retained as usize,
        columns,
        logical_columns: index.table.len() * 25 + index.table.folder_count() * 20,
        logical_names: (index.table.len() - 1) * 18 + 3,
        frn: index.ids.memory_bytes(),
        queue,
        waiters,
        pending_count: pending.blocked.len(),
    }
}

/// Mirrors enumeration with phase checkpoints; a production run below cross-checks its peak.
fn instrument(files: u32, hint: SizeHint, old: usize, probe: &Probe) -> (NtfsIndex, Parts) {
    let mut volume = DeepMft::new(files, false);
    let mut index = NtfsIndex::new(
        'C',
        JournalPosition {
            journal_id: 7,
            next_usn: 100,
        },
    )
    .unwrap();
    index
        .table
        .try_reserve_total(hint.entries, hint.folders, hint.name_bytes)
        .unwrap();
    let mut pending = Pending::default();
    let mut best = Parts::default();
    let mut last_peak = 0;
    while let Some(bytes) = volume.enumerate(0).unwrap() {
        let batch = parse_batch(&bytes).unwrap();
        for record in batch.records {
            add_record(&mut index, &mut pending, record);
        }
        let reading = probe.read();
        if reading.peak > last_peak {
            best = parts(&index, &pending, probe);
            last_peak = best.peak;
        }
    }
    if hint == SizeHint::default() {
        index.table.trim_slack();
    }
    index.ids.shrink_to_fit();
    let before_finish = probe.read().peak;
    assert_eq!(pending.blocked_peak, files as usize * 3 / 10);
    assert!(pending.finish().is_empty());
    assert_eq!(index.table.len(), (files + files / 10 + 1) as usize);
    println!("INSTR files={files} old={old} max={before_finish} sampled={best:?}");
    (index, best)
}

/// Applies the same add and size sequence as enumeration, without platform size I/O.
fn add_record(index: &mut NtfsIndex, pending: &mut Pending, record: crate::usn::UsnRecord) {
    let file_id = record.file_id;
    let folder = record.flags.contains(Flags::FOLDER);
    pending
        .push(
            index,
            TableChange::Add {
                file_id,
                parent_id: record.parent_id,
                name: record.name,
                flags: record.flags,
            },
        )
        .unwrap();
    if !folder {
        pending
            .push(index, TableChange::Size { file_id, bytes: 1 })
            .unwrap();
    }
}

/// Runs the production path, with the same buffer construction and no measurement callbacks.
fn build(files: u32, ordered: bool, hint: SizeHint) -> Enumeration {
    enumerate_volume_sized(
        &mut DeepMft::new(files, ordered),
        'C',
        usize::MAX,
        hint,
        &CancelToken::new(),
    )
    .unwrap()
}

/// Reports peak ownership and returns the peak and final held bytes.
fn instrument_report(
    files: u32,
    hint: SizeHint,
    old_bytes: usize,
    probe: &Probe,
) -> (usize, usize) {
    let start = Instant::now();
    let (index, sampled) = instrument(files, hint, old_bytes, probe);
    let peak = probe.read().peak;
    println!(
        "HELD columns={:?} frn={} peak={peak} ms={} sampled_peak={} live={} logical_columns={} logical_names={} sampled_frn={} queue={} waiters={} blocked={} sampled_columns={:?}",
        index.table.column_bytes(),
        index.ids.memory_bytes(),
        start.elapsed().as_millis(),
        sampled.peak,
        sampled.live,
        sampled.logical_columns,
        sampled.logical_names,
        sampled.frn,
        sampled.queue,
        sampled.waiters,
        sampled.pending_count,
        sampled.columns
    );
    (peak, held(index, probe))
}

/// Measures parent-first construction; the old search-only term is an arithmetic projection.
fn ordered_report(files: u32, hint: SizeHint, old: [usize; 2], probe: &Probe) {
    probe.mark();
    let start = Instant::now();
    let ordered = build(files, true, hint);
    let ordered_peak = probe.read().peak;
    assert_eq!(ordered.blocked_peak, 0);
    assert!(ordered.unresolved.is_empty());
    let [old_bytes, answer_bytes] = old;
    println!(
        "ALTERNATIVE files={files} ordered_peak={ordered_peak} old={old_bytes} answer_only={answer_bytes} full_old_peak={} answer_old_peak={} ms={}",
        old_bytes + ordered_peak,
        answer_bytes + ordered_peak,
        start.elapsed().as_millis()
    );
    drop(ordered);
}

#[test]
#[ignore = "throwaway checker measurement"]
fn checker_peak_breakdown() {
    let files: u32 = std::env::var("CHECKER_FILES")
        .unwrap_or_else(|_| "1000000".into())
        .parse()
        .unwrap();
    for rebuild in [false, true] {
        let probe = Probe::start();
        let old = rebuild.then(|| build(files, true, SizeHint::default()).index);
        let old_bytes = old.as_ref().map_or(0, |index| {
            index.table.memory_bytes() + index.ids.memory_bytes()
        });
        let hint = old.as_ref().map_or_else(SizeHint::default, super::hint);
        probe.mark();
        let (peak, steady) = instrument_report(files, hint, old_bytes, &probe);
        probe.mark();
        let start = Instant::now();
        let production = build(files, false, hint);
        let actual = probe.read().peak;
        assert!(production.unresolved.is_empty());
        assert_eq!(production.blocked_peak, files as usize * 3 / 10);
        assert!(
            actual.abs_diff(peak) < peak / 100,
            "instrumentation changed the peak"
        );
        println!(
            "PRODUCTION files={files} rebuild={rebuild} old={old_bytes} peak={actual} steady={steady} ratio={:.3} ms={}",
            actual as f64 / steady as f64,
            start.elapsed().as_millis()
        );
        let answer_bytes = old.as_ref().map_or(0, |index| {
            index
                .table
                .column_bytes()
                .into_iter()
                .filter(|(name, _)| ["names", "offsets", "parents", "flags"].contains(name))
                .map(|(_, bytes)| bytes)
                .sum::<usize>()
        });
        drop(production);
        drop(old);
        if rebuild {
            ordered_report(files, hint, [old_bytes, answer_bytes], &probe);
            if std::env::var_os("CHECKER_ASSERT_TARGET").is_some() {
                assert!(
                    actual as f64 / old_bytes as f64 <= 1.7,
                    "strict rebuild ceiling exceeded"
                );
            }
        }
    }
}
