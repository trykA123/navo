//! Journal rejection and live cross-pass churn scripts for the two-pass builder.
use super::*;
use crate::ntfs::test_support::{SyntheticRecord, append_record, journal_metadata};
use crate::pipe::CancelToken;
const ROOT: u64 = (5 << 48) | 5;
/// Encodes MFT or journal records with the supplied next cursor and reason flags.
fn wire(next: u64, records: &[(u64, u64, bool, &'static str, u32)]) -> Vec<u8> {
    let mut bytes = next.to_le_bytes().to_vec();
    for &(file_id, parent_id, folder, name, reason) in records {
        let offset = bytes.len();
        let spec = SyntheticRecord {
            file_id,
            parent_id,
            folder,
            name,
        };
        append_record(&mut bytes, spec);
        bytes[offset + 24..offset + 32].copy_from_slice(&100i64.to_le_bytes());
        bytes[offset + 40..offset + 44].copy_from_slice(&reason.to_le_bytes());
    }
    bytes
}
#[derive(Default)]
struct Volume {
    pass: u8,
    when: u8,
    invalid: bool,
    sizes: usize,
    journal: Vec<u8>,
}
impl Volume {
    /// Mutates the live MFT and journal during pass one, between reads, or during pass two.
    fn inject(&mut self, when: u8) {
        if self.when == when && self.journal.is_empty() {
            let parent = if self.invalid { 99 } else { 9 };
            let records = [
                (23, ROOT, false, "new", 0x100),
                (20, 8, false, "gone", 0x200),
                (21, 8, false, "old", 0x1000),
                (21, 8, false, "renamed", 0x2000),
                (8, ROOT, true, "a", 0x1000),
                (8, parent, true, "moved", 0x2000),
            ];
            self.journal = wire(120, &records);
        }
    }
}
impl VolumeIo for Volume {
    /// Injects between reads and reports journal continuity from 100 through 120.
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        if self.pass == 1 {
            self.inject(2);
        }
        let mut bytes = journal_metadata(7);
        let next = if self.journal.is_empty() { 100i64 } else { 120 };
        bytes[16..24].copy_from_slice(&next.to_le_bytes());
        Ok(bytes)
    }
    /// Encodes the current live MFT once per cursor-zero start, with a separate EOF request.
    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        if cursor != 0 {
            return Ok(None);
        }
        self.pass += 1;
        self.inject(self.pass * 2 - 1);
        let mut rows = [
            (8, ROOT, true, "a", 0),
            (9, ROOT, true, "b", 0),
            (20, 8, false, "gone", 0),
            (21, 8, false, "old", 0),
        ];
        if !self.journal.is_empty() {
            rows[0] = (8, if self.invalid { 99 } else { 9 }, true, "moved", 0);
            rows[2] = (23, ROOT, false, "new", 0);
            rows[3].3 = "renamed";
        }
        Ok(Some(wire(1, &rows)))
    }
    /// Returns the changes injected since the pre-pass-one journal position.
    fn read_journal(&mut self, _: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        Ok(self.journal.clone())
    }
    /// Counts size reads; follow-only fixtures return FileGone, builds return ten bytes.
    fn file_size(&mut self, _: u128) -> Result<u64, NtfsError> {
        self.sizes += 1;
        (self.pass > 0).then_some(10).ok_or(NtfsError::FileGone)
    }
}
/// Builds through production enumeration and replays churn from the original journal cursor.
fn build(volume: &mut Volume) -> Result<NtfsIndex, NtfsError> {
    let mut built = enumerate_volume(volume, 'C', usize::MAX, &CancelToken::new())?;
    assert_eq!(volume.pass, 2, "churn must span both production reads");
    let batch = read_volume_journal(volume, built.index.position)?;
    built.unresolved.extend(built.index.apply_batch(batch)?);
    if !built.unresolved.is_empty() || !built.index.pending_renames().is_empty() {
        return Err(NtfsError::Reenumerate);
    }
    Ok(built.index)
}
#[test]
fn journal_churn_hard_links_require_reenumeration_before_size_refresh() {
    for reason in [0x10000, 0x10100] {
        let mut volume = Volume {
            journal: wire(120, &[(20, ROOT, false, "link", reason)]),
            ..Volume::default()
        };
        let saved = JournalState::parse(&journal_metadata(7)).unwrap().position;
        let error = read_volume_journal(&mut volume, saved).err();
        let rejected = matches!(error, Some(NtfsError::Reenumerate));
        assert_eq!((reason, rejected, volume.sizes), (reason, true, 0));
    }
}
#[test]
fn journal_churn_reconciles_paths_totals_or_rejects_invalidated_parents() {
    for when in 1..=3 {
        for invalid in [false, true] {
            let mut volume = Volume {
                when,
                invalid,
                ..Volume::default()
            };
            let result = build(&mut volume);
            if invalid {
                assert!(matches!(result, Err(NtfsError::Reenumerate)));
                continue;
            }
            let index = result.unwrap();
            let path = index.table.path_with(index.ids[&21], '\\').unwrap();
            assert_eq!(path, "C:\\b\\moved\\renamed");
            assert_eq!(index.table.parent(index.ids[&23]), Some(0));
            let totals = index.table.totals(0).unwrap();
            assert_eq!((totals.bytes, totals.files, totals.folders), (20, 2, 2));
            assert_eq!(index.position.next_usn, 120);
        }
    }
}
