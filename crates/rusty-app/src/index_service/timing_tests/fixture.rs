use std::path::{Path, PathBuf};
use std::time::Duration;

use rusty_index::file::{self, Header};
use rusty_index::ntfs::NtfsIndex;
use rusty_index::pipe::{QueryTerm, Reply, Request, VolumeStatus};
use rusty_index::table::{Flags, NO_PARENT, NameTable};
use rusty_index::usn::{FrnMap, UsnBatch, UsnRecord};

use super::super::answer::{SharedVolume, State, answer};

pub(super) const ENTRIES: u32 = 1_000_000;
pub(super) const RECORDS: u32 = 200_000;
pub(super) const ANSWER_BUDGET: Duration = Duration::from_millis(50);
pub(super) const WAIT: Duration = Duration::from_secs(30);

pub(super) fn header() -> Header {
    Header {
        volume_serial: 42,
        journal_id: 7,
        next_usn: 100,
        root: "C:".into(),
        built_at: 0,
    }
}

pub(super) fn synthetic_index() -> NtfsIndex {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let mut ids = FrnMap::with_capacity(ENTRIES as usize);
    ids.insert(5, root);
    for number in 1..ENTRIES {
        let name = if number == ENTRIES - 1 {
            "needle.txt".into()
        } else {
            format!("entry-{number:07}.txt")
        };
        let id = table.push(root, &name, Flags::NONE).unwrap();
        ids.insert(u128::from(number) + 100, id);
    }
    NtfsIndex::restore((header(), table, ids), 42, 7).unwrap()
}

pub(super) fn snapshot() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("c.index");
    let index = synthetic_index();
    save(&path, &index);
    (directory, path)
}

pub(super) fn save(path: &Path, index: &NtfsIndex) {
    let (table, ids, position) = index.snapshot().unwrap();
    let header = Header {
        journal_id: position.journal_id,
        next_usn: position.next_usn,
        ..header()
    };
    file::save_with_ids(path, &header, table, ids).unwrap();
}

pub(super) fn batch(records: u32) -> UsnBatch {
    UsnBatch {
        next_usn: 100 + i64::from(records),
        records: (1..=records)
            .map(|number| UsnRecord {
                file_id: u128::from(number) + 100,
                parent_id: 5,
                usn: 100 + i64::from(number),
                reason: 0x8000,
                flags: Flags::NONE,
                name: format!("entry-{number:07}.txt"),
                size: Some(u64::from(number)),
            })
            .collect(),
    }
}

pub(super) fn volume(index: Option<NtfsIndex>) -> SharedVolume {
    SharedVolume::new(State {
        status: VolumeStatus {
            root: "C:".into(),
            entries: index.as_ref().map_or(0, |index| index.table.len() as u64),
            progress: index.is_none().then_some(0),
            ..VolumeStatus::default()
        },
        index: index.map(std::sync::Arc::new),
        ..State::default()
    })
}

pub(super) fn query() -> Request {
    Request {
        terms: vec![QueryTerm::Name("needle".into())],
    }
}

pub(super) fn timed_answer(volume: &SharedVolume, request: &Request) -> (Reply, Duration) {
    rusty_index::pipe::test_support::timed_answer(request, &|request, cancel| {
        answer(std::slice::from_ref(volume), request, cancel)
    })
}

#[allow(
    clippy::print_stdout,
    reason = "IX1 reports measured latency and lock duration"
)]
pub(super) fn report(case: &str, metric: &str, elapsed: Duration) {
    println!(
        "IX1 {case}: {metric} = {:.3} ms ({ENTRIES} entries)",
        elapsed.as_secs_f64() * 1000.0
    );
}

pub(super) fn within(case: &str, elapsed: Duration, budget: Duration) {
    assert!(
        elapsed <= budget,
        "IX1 {case}: {elapsed:?} exceeds {budget:?}"
    );
}
