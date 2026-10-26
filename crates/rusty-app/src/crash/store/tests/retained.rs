use super::*;

use rusty_core::ProviderPath;
use rusty_core::retention::{DAY_MS, RetainedKind, RetainedNode, RetentionRecord};
use rusty_fs::{Provider, ProviderOps};

fn saved(data: &Path, fs: &FakeFs, owner: bool) -> RetentionRecord {
    fs.add_file("home/deleted", 7);
    let source = fs.location("home/deleted");
    let container = fs.location("home/.ferrite-retained-a");
    let identity = fs.file_identity(&source).unwrap().unwrap();
    fs.create_folder(&container).unwrap();
    let record = RetentionRecord {
        version: 1,
        payload_name: None,
        source: source.clone(),
        container: container.clone(),
        parent_identity: fs
            .file_identity(&source.parent().unwrap())
            .unwrap()
            .unwrap(),
        container_identity: fs.file_identity(&container).unwrap(),
        nodes: vec![RetainedNode {
            path: vec![],
            kind: RetainedKind::File,
            identity,
            size: 7,
            modified_ms: None,
        }],
        created_at: 0,
        expires_at: DAY_MS,
    };
    fs.rename(&source, &record.held().unwrap()).unwrap();
    let (db, _) = Db::open(&database(data), false).unwrap();
    if owner {
        db.connection()
            .execute(
                "INSERT INTO job VALUES (1, 'delete', 'running', 0, X'00')",
                [],
            )
            .unwrap();
    }
    db.connection()
        .execute(
            "INSERT INTO retained_entry (job_id, state, record) VALUES (?1, 'ready', ?2)",
            (owner.then_some(1), serde_json::to_vec(&record).unwrap()),
        )
        .unwrap();
    record
}

#[test]
fn a_verified_retention_catalog_survives_database_recovery_without_touching_payloads() {
    let data = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let record = saved(data.path(), &fs, false);
    damaged(data.path());
    let start = check_with(Some(data.path()), true, &fs, true);
    assert!(start.notice.is_some());
    let (db, _) = crate::crash::open_db(&database(data.path())).unwrap();
    let catalog = rusty_store::RetentionCatalog::new(db).unwrap();
    assert_eq!(catalog.list_after(0, 10).unwrap()[0].record, record);
    assert!(fs.metadata(&record.held().unwrap()).is_ok());
}

#[test]
fn a_rejected_owner_journal_keeps_retained_files_and_shows_manual_recovery_paths() {
    let data = tempfile::tempdir().unwrap();
    let fs = FakeFs::new();
    let record = saved(data.path(), &fs, true);
    damaged(data.path());
    let start = check_with(Some(data.path()), true, &fs, true);
    let notice = start.notice.unwrap().content();
    assert!(
        notice
            .details
            .contains(&record.container.as_str().to_owned())
    );
    assert!(
        notice
            .details
            .contains(&rusty_l10n::tr("store-recovery-retained-manual"))
    );
    let details = std::fs::read_to_string(data.path().join("navo-recovery-1-1.jsonl")).unwrap();
    assert!(details.contains(record.container.as_str()));
    assert!(details.contains(record.source.as_str()));
    assert!(fs.metadata(&record.held().unwrap()).is_ok());
    let state = State::load(data.path()).unwrap();
    assert!(
        state.notices[0]
            .reset
            .iter()
            .any(|table| table == "retained_entry")
    );
    let (db, _) = crate::crash::open_db(&database(data.path())).unwrap();
    assert!(
        rusty_store::RetentionCatalog::new(db)
            .unwrap()
            .list_after(0, 10)
            .unwrap()
            .is_empty()
    );
    assert!(
        fs.metadata(
            &record
                .container
                .join(&ProviderPath::from_text("payload"))
                .unwrap()
        )
        .is_ok()
    );
}
