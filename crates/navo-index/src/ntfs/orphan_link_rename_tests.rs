//! AF9: a rename of a hard-link name inside a folder the build skipped never resolves.

use super::journal_churn_tests::wire;
use super::*;
use crate::ntfs::test_support::journal_metadata;
use crate::pipe::CancelToken;

const ROOT: u64 = (5 << 48) | 5;
const OLD_NAME: u32 = 0x1000;
const NEW_NAME: u32 = 0x2000;

/// Volume whose folder 8 has a parent (99) outside the MFT, so every build skips it.
struct Volume {
    journal: Vec<u8>,
    queries: usize,
}

impl VolumeIo for Volume {
    /// Reports cursor 100 during the build (two queries), then 120 once the rename is journaled.
    fn query_journal(&mut self) -> Result<Vec<u8>, NtfsError> {
        self.queries += 1;
        let next: i64 = if self.queries > 2 { 120 } else { 100 };
        let mut bytes = journal_metadata(7);
        bytes[16..24].copy_from_slice(&next.to_le_bytes());
        Ok(bytes)
    }

    /// File 20 has two hard-link names: one under the root, one in the orphan folder 8.
    fn enumerate(&mut self, cursor: u64) -> Result<Option<Vec<u8>>, NtfsError> {
        let rows = [
            (8, 99, true, "orphan", 0),
            (20, ROOT, false, "a.txt", 0),
            (20, 8, false, "b.txt", 0),
        ];
        Ok((cursor == 0).then(|| wire(1, &rows)))
    }

    /// Replays the rename of the second link, as the journal does after each build.
    fn read_journal(&mut self, _: JournalPosition) -> Result<Vec<u8>, NtfsError> {
        Ok(self.journal.clone())
    }

    fn file_size(&mut self, _: u128) -> Result<u64, NtfsError> {
        Ok(10)
    }
}

/// A rename into a skipped folder is unresolved after every rebuild, so the loop never ends.
#[test]
fn a_rename_inside_a_folder_the_build_skipped_is_not_left_unresolved_after_a_rebuild() {
    let journal = wire(
        120,
        &[
            (20, 8, false, "b.txt", OLD_NAME),
            (20, 8, false, "c.txt", NEW_NAME),
        ],
    );
    for build in 1..=2 {
        let mut volume = Volume {
            journal: journal.clone(),
            queries: 0,
        };
        let mut built =
            enumerate_volume(&mut volume, 'C', usize::MAX, &CancelToken::new()).expect("build");
        let batch = read_volume_journal(&mut volume, built.index.position).expect("journal");
        built
            .unresolved
            .extend(built.index.apply_batch(batch).expect("apply"));
        assert!(
            built.unresolved.is_empty(),
            "build {build} left the rename unresolved: {:?}",
            built.unresolved
        );
    }
}
