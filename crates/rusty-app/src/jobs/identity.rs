use rusty_core::{FsError, FsErrorKind};
use rusty_fs::{FileIdentity, FileIdentityKind, FileStamp};
use rusty_store::{JobId, StepState};
use serde::{Deserialize, Serialize};

use super::{SharedJournal, lock};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    volume: u64,
    file: [u8; 16],
    #[serde(default, skip_serializing_if = "is_false")]
    legacy_windows_index: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fingerprint: Option<Shape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stamp: Option<Stamp>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stamp {
    size: u64,
    modified_ms: Option<i64>,
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Shape {
    File,
    Folder,
}

pub(super) fn record(
    journal: &SharedJournal,
    job: JobId,
    (seq, identity): (u32, FileIdentity),
) -> Result<(), FsError> {
    record_stamped(journal, job, (seq, identity, None))
}

pub(super) fn record_stamped(
    journal: &SharedJournal,
    job: JobId,
    (seq, identity, stamp): (u32, FileIdentity, Option<FileStamp>),
) -> Result<(), FsError> {
    let fingerprint = match identity.kind {
        FileIdentityKind::Fingerprint { folder: true } => Some(Shape::Folder),
        FileIdentityKind::Fingerprint { folder: false } => Some(Shape::File),
        FileIdentityKind::Native | FileIdentityKind::WindowsIndex => None,
    };
    let bytes = serde_json::to_vec(&Record {
        version: if fingerprint.is_some() { 2 } else { 1 },
        volume: identity.volume,
        file: identity.file,
        legacy_windows_index: identity.kind == FileIdentityKind::WindowsIndex,
        fingerprint,
        stamp: stamp.map(|stamp| Stamp {
            size: stamp.size,
            modified_ms: stamp.modified_ms,
        }),
    })
    .map_err(|_| failure())?;
    lock(journal)
        .mark_step(job, seq, StepState::Pending, Some(&bytes))
        .map_err(|_| failure())
}

pub(super) fn read(
    journal: &SharedJournal,
    job: JobId,
    seq: u32,
) -> Result<Option<FileIdentity>, FsError> {
    let Some(bytes) = lock(journal).step_detail(job, seq).map_err(|_| failure())? else {
        return Ok(None);
    };
    decode(&bytes)
}

pub(super) fn read_stamp(
    journal: &SharedJournal,
    job: JobId,
    seq: u32,
) -> Result<Option<FileStamp>, FsError> {
    let Some(bytes) = lock(journal).step_detail(job, seq).map_err(|_| failure())? else {
        return Ok(None);
    };
    if bytes == b"undone" {
        return Ok(None);
    }
    decode(&bytes)?;
    let record: Record = serde_json::from_slice(&bytes).map_err(|_| failure())?;
    Ok(record.stamp.map(|stamp| FileStamp {
        size: stamp.size,
        modified_ms: stamp.modified_ms,
    }))
}

pub(super) fn decode(bytes: &[u8]) -> Result<Option<FileIdentity>, FsError> {
    if bytes == b"undone" {
        return Ok(None);
    }
    let record: Record = serde_json::from_slice(bytes).map_err(|_| failure())?;
    let kind = match (
        record.version,
        record.legacy_windows_index,
        record.fingerprint,
    ) {
        (1, false, None) => FileIdentityKind::Native,
        (1, true, None) => FileIdentityKind::WindowsIndex,
        (2, false, Some(shape)) => FileIdentityKind::Fingerprint {
            folder: matches!(shape, Shape::Folder),
        },
        _ => return Err(failure()),
    };
    Ok(Some(FileIdentity {
        volume: record.volume,
        file: record.file,
        kind,
    }))
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if takes a reference"
)]
fn is_false(value: &bool) -> bool {
    !value
}

fn failure() -> FsError {
    FsError::new(FsErrorKind::Unknown)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rusty_core::{OpKind, OperationPlan, Step, Totals};
    use rusty_fs::{FakeFs, FileIdentity, FileIdentityKind, JobOutcome, StepOutcome};
    use rusty_store::StepState;
    use rusty_ui::browser::OpLog;

    use crate::jobs::{SharedJournal, lock, log_factory, open, undo_log};

    const IDENTITY: FileIdentity = FileIdentity {
        volume: u64::MAX,
        file: [255; 16],
        kind: FileIdentityKind::Native,
    };

    fn log(journal: &SharedJournal) -> Box<dyn OpLog> {
        let fs = FakeFs::new();
        log_factory(Arc::clone(journal))(&OperationPlan {
            kind: OpKind::Rename,
            steps: vec![Step::Rename {
                source: fs.location("a"),
                target: fs.location("b"),
            }],
            totals: Totals {
                files: 1,
                folders: 0,
                bytes: 0,
            },
            conflicts: Vec::new(),
            risks: Vec::new(),
        })
        .unwrap()
    }

    #[test]
    fn a_prepared_identity_is_durable_before_the_rename_runs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("navo.db");
        let journal = open(&path).unwrap();
        let mut log = log(&journal);
        log.prepare_rename(0, IDENTITY).unwrap();
        let jobs = lock(&journal).unfinished().unwrap();
        assert_eq!(jobs[0].steps[0].state, StepState::Pending);
        drop((log, journal));
        let reopened = open(&path).unwrap();
        let mut log = undo_log(&reopened, jobs[0].id).unwrap();
        assert_eq!(log.rename_identity(0).unwrap(), Some(IDENTITY));
        log.step(0, StepOutcome::Done).unwrap();
        assert_eq!(log.rename_identity(0).unwrap(), Some(IDENTITY));
    }

    #[test]
    fn a_finished_batch_keeps_its_identity_after_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("navo.db");
        let journal = open(&path).unwrap();
        let mut log = log(&journal);
        log.prepare_rename(0, IDENTITY).unwrap();
        log.step(0, StepOutcome::Done).unwrap();
        log.finish(&JobOutcome::Done, &FakeFs::new()).unwrap();
        drop((log, journal));
        let reopened = open(&path).unwrap();
        let job = lock(&reopened).last_undoable("rename").unwrap().unwrap();
        let log = undo_log(&reopened, job.id).unwrap();
        assert_eq!(log.rename_identity(0).unwrap(), Some(IDENTITY));
    }

    #[test]
    fn a_legacy_windows_identity_preserves_its_domain_after_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("navo.db");
        let journal = open(&path).unwrap();
        let identity = FileIdentity {
            volume: 7,
            file: u128::from(u64::MAX).to_le_bytes(),
            kind: FileIdentityKind::WindowsIndex,
        };
        let mut log = log(&journal);
        log.prepare_rename(0, identity).unwrap();
        let job = lock(&journal).unfinished().unwrap()[0].id;
        drop((log, journal));
        let reopened = open(&path).unwrap();
        let log = undo_log(&reopened, job).unwrap();
        assert_eq!(log.rename_identity(0).unwrap(), Some(identity));
        assert_ne!(
            log.rename_identity(0).unwrap(),
            Some(FileIdentity {
                kind: FileIdentityKind::Native,
                ..identity
            })
        );
    }

    fn stored(detail: &[u8]) -> Result<Option<FileIdentity>, rusty_core::FsError> {
        let dir = tempfile::tempdir().unwrap();
        let journal = open(&dir.path().join("navo.db")).unwrap();
        let log = log(&journal);
        let job = lock(&journal).unfinished().unwrap()[0].id;
        lock(&journal)
            .mark_step(job, 0, StepState::Done, Some(detail))
            .unwrap();
        log.rename_identity(0)
    }

    #[test]
    fn version_one_records_written_before_fingerprints_still_parse() {
        let file = format!("{:?}", [3_u8; 16]).replace(' ', "");
        let native = format!(r#"{{"version":1,"volume":7,"file":{file}}}"#);
        let legacy =
            format!(r#"{{"version":1,"volume":7,"file":{file},"legacy_windows_index":true}}"#);
        for (json, kind) in [
            (native, FileIdentityKind::Native),
            (legacy, FileIdentityKind::WindowsIndex),
        ] {
            assert_eq!(
                stored(json.as_bytes()).unwrap(),
                Some(FileIdentity {
                    volume: 7,
                    file: [3; 16],
                    kind,
                })
            );
        }
    }

    #[test]
    fn a_fingerprint_keeps_its_own_domain_after_reopening() {
        for folder in [false, true] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("navo.db");
            let journal = open(&path).unwrap();
            let identity = FileIdentity {
                volume: 0,
                file: [4; 16],
                kind: FileIdentityKind::Fingerprint { folder },
            };
            let mut log = log(&journal);
            log.prepare_rename(0, identity).unwrap();
            let job = lock(&journal).unfinished().unwrap()[0].id;
            drop((log, journal));
            let reopened = open(&path).unwrap();
            let log = undo_log(&reopened, job).unwrap();
            assert_eq!(log.rename_identity(0).unwrap(), Some(identity));
        }
    }

    #[test]
    fn a_record_whose_version_does_not_match_its_fields_is_an_error() {
        let file = format!("{:?}", [3_u8; 16]).replace(' ', "");
        for json in [
            format!(r#"{{"version":1,"volume":0,"file":{file},"fingerprint":"file"}}"#),
            format!(r#"{{"version":2,"volume":0,"file":{file}}}"#),
            format!(r#"{{"version":3,"volume":0,"file":{file},"fingerprint":"file"}}"#),
        ] {
            assert!(stored(json.as_bytes()).is_err(), "{json}");
        }
    }

    #[test]
    fn an_unreadable_identity_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let journal = open(&dir.path().join("navo.db")).unwrap();
        let log = log(&journal);
        let job = lock(&journal).unfinished().unwrap()[0].id;
        lock(&journal)
            .mark_step(job, 0, StepState::Done, Some(b"not json"))
            .unwrap();
        assert!(log.rename_identity(0).is_err());
    }
}
