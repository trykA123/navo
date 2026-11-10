use navo_core::{FileStamp, write_plan};
use navo_fs::{EntryKind, FsEntry, JobContext, run_job};

use super::*;

fn file(name: &str, (size, modified): (u64, i64)) -> FsEntry {
    FsEntry {
        name: navo_core::ProviderPath::from_text(name),
        kind: EntryKind::File,
        size,
        modified_ms: Some(modified),
        attributes: navo_fs::Attributes::default(),
    }
}

fn save(profile: &Profile) {
    profile.fs.add_entry("home", file("a.txt", (8, 1_000)));
    profile.fs.set_hash("home/a.txt", [1; 32]);
    profile
        .fs
        .add_entry("home", file(".a.txt.part", (11, 2_000)));
    let seen = FileStamp {
        size: 8,
        modified_ms: Some(1_000),
        hash: [1; 32],
    };
    let plan = write_plan(
        &profile.at("home/a.txt"),
        &profile.at("home/.a.txt.part"),
        seen,
        11,
    );
    let ops = profile.fs.as_ref() as &dyn ProviderOps;
    let factory = crate::jobs::experimental_factory(profile.journal.clone(), profile.dirs.clone());
    let mut log = factory(&plan).unwrap();
    log.check(LoggedOperation {
        plan: &plan,
        ops,
        cancel: &CancelToken::new(),
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: None,
    })
    .unwrap();
    let outcome = run_job(
        &plan,
        &[],
        &mut JobContext {
            ops,
            log: log.as_mut(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
    log.finish(&outcome, ops).unwrap();
}

#[test]
fn maintenance_timer_pass_expires_a_saved_old_version_but_not_a_fresh_one() {
    let profile = Profile::new();
    save(&profile);
    assert_eq!(profile.live(), 1);
    let container = profile.entries()[0].record.container.clone();
    let now = super::super::super::super::now_ms();
    assert!(matches!(profile.pass(now), Pass::Ran { purged: 0, .. }));
    assert_eq!(profile.live(), 1);
    assert_eq!(
        profile.pass(now + 4 * DAY_MS),
        Pass::Ran {
            volumes: 1,
            purged: 1,
            bytes: 8,
            pruned: 0
        }
    );
    assert_eq!(profile.live(), 0);
    assert!(profile.fs.metadata(&container).is_err());
}

#[test]
fn trimming_a_saved_old_version_forgets_the_save_undo_rows() {
    let profile = Profile::new();
    save(&profile);
    let holding = || {
        crate::jobs::lock(&profile.journal)
            .finished_holding()
            .unwrap()
    };
    assert_eq!(holding().len(), 1, "a retained save keeps its undo row");
    let now = super::super::super::super::now_ms();
    assert!(matches!(
        profile.pass(now + 4 * DAY_MS),
        Pass::Ran { purged: 1, .. }
    ));
    assert_eq!(holding(), [], "the trimmed save keeps no undo row");
}
