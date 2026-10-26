use super::*;

use rusty_store::{SalvageOutcome, quarantine_database, salvage_database};

#[test]
fn a_verified_salvaged_journal_rolls_back_the_interrupted_copy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    let fs = fake();
    let plan = copy_plan(&fs);
    let journal = open(&path).unwrap();
    run(&fs, &plan, &journal, false);
    drop(journal);
    crate::crash::test_damage::fill_history(&path);
    crate::crash::test_damage::corrupt_middle_page(&path);
    let backup = quarantine_database(&path)
        .unwrap()
        .unwrap()
        .database
        .unwrap();
    let (fresh, _) = Db::open(&path, false).unwrap();
    let report = salvage_database(&backup, &fresh).unwrap();
    assert!(
        report.tables.iter().any(|table| table.name == "job"
            && matches!(table.outcome, SalvageOutcome::Copied { rows: 1 }))
    );
    let recovered = Arc::new(Mutex::new(JobJournal::new(fresh).unwrap()));
    assert!(exists(&fs, "dest/docs/report.txt"));
    assert_eq!(recover(&recovered, &fs).unwrap(), 1);
    assert!(!exists(&fs, "dest/docs"));
    assert!(exists(&fs, "home/docs/report.txt"));
}

#[test]
fn a_rejected_salvaged_journal_never_rolls_back_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    let fs = fake();
    let plan = copy_plan(&fs);
    let journal = open(&path).unwrap();
    run(&fs, &plan, &journal, false);
    drop(journal);
    let page = crate::crash::test_damage::root_page(&path, "job_step");
    crate::crash::test_damage::corrupt_page(&path, page);
    let backup = quarantine_database(&path)
        .unwrap()
        .unwrap()
        .database
        .unwrap();
    let (fresh, _) = Db::open(&path, false).unwrap();
    let report = salvage_database(&backup, &fresh).unwrap();
    assert!(!report.jobs.is_empty());
    let recovered = Arc::new(Mutex::new(JobJournal::new(fresh).unwrap()));
    assert_eq!(recover(&recovered, &fs).unwrap(), 0);
    assert!(exists(&fs, "dest/docs/report.txt"));
    assert!(exists(&fs, "home/docs/report.txt"));
}
