use super::*;
use rusty_core::retention::{DAY_MS, RetainedKind, RetainedNode, RetainedState, RetentionRecord};
use rusty_core::{FileIdentity, Location};

fn record() -> RetentionRecord {
    let identity = FileIdentity {
        kind: rusty_core::FileIdentityKind::Native,
        volume: 1,
        file: [1; 16],
    };
    RetentionRecord {
        version: 1,
        payload_name: None,
        source: Location::new("/sample/a").unwrap(),
        container: Location::new("/sample/.ferrite-retained-a").unwrap(),
        parent_identity: identity,
        container_identity: None,
        nodes: vec![RetainedNode {
            path: vec![],
            kind: RetainedKind::File,
            identity,
            size: 7,
            modified_ms: Some(9),
        }],
        created_at: 0,
        expires_at: DAY_MS,
    }
}

fn catalog() -> RetentionCatalog {
    RetentionCatalog::new(Db::open_in_memory().unwrap()).unwrap()
}

fn insert(catalog: &mut RetentionCatalog) -> RetentionEntry {
    let mut tx = catalog.transaction().unwrap();
    let entry = tx.insert(&record(), None).unwrap();
    tx.commit().unwrap();
    entry
}

#[test]
fn catalog_records_survive_closing_and_reopening_the_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    let (db, _) = Db::open(&path, false).unwrap();
    let mut catalog = RetentionCatalog::new(db).unwrap();
    let entry = insert(&mut catalog);
    drop(catalog);
    let (db, _) = Db::open(&path, true).unwrap();
    assert_eq!(
        RetentionCatalog::new(db).unwrap().get(entry.id).unwrap(),
        entry
    );
}

#[test]
fn catalog_reads_are_paginated_by_identifier() {
    let mut catalog = catalog();
    let first = insert(&mut catalog);
    let second = insert(&mut catalog);
    assert_eq!(catalog.list_after(0, 1).unwrap(), vec![first.clone()]);
    assert_eq!(catalog.list_after(first.id.0, 1).unwrap(), vec![second]);
    assert!(catalog.list_after(0, 0).is_err());
    assert!(catalog.list_after(-1, 1001).is_err());
}

#[test]
fn a_rolled_back_transaction_never_creates_a_capture_intent() {
    let mut catalog = catalog();
    {
        let mut tx = catalog.transaction().unwrap();
        tx.insert(&record(), None).unwrap();
    }
    assert!(catalog.list_after(0, 10).unwrap().is_empty());
}

#[test]
fn stale_transitions_and_snapshot_changes_are_refused() {
    let mut catalog = catalog();
    let mut entry = insert(&mut catalog);
    entry.record.container_identity = Some(entry.record.parent_identity);
    entry.state = RetainedState::Ready;
    let mut tx = catalog.transaction().unwrap();
    assert!(tx.save(&entry, RetainedState::Ready).is_err());
    entry.record.nodes[0].size += 1;
    assert!(tx.save(&entry, RetainedState::Preparing).is_err());
}

#[test]
fn malformed_json_is_rejected_before_it_reaches_the_engine() {
    let db = Db::open_in_memory().unwrap();
    db.connection()
        .execute(
            "INSERT INTO retained_entry (state, record) VALUES ('preparing', ?1)",
            [b"not json".as_slice()],
        )
        .unwrap();
    let catalog = RetentionCatalog::new(db).unwrap();
    assert!(catalog.list_after(0, 10).is_err());
}

#[test]
fn owned_captures_pin_active_jobs_and_prevent_journal_pruning() {
    let db = Db::open_in_memory().unwrap();
    db.connection()
        .execute(
            "INSERT INTO job VALUES (1, 'delete', 'running', 0, X'00')",
            [],
        )
        .unwrap();
    let mut catalog = RetentionCatalog::new(db).unwrap();
    let mut tx = catalog.transaction().unwrap();
    let mut entry = tx.insert(&record(), Some(crate::JobId(1))).unwrap();
    assert!(entry.pinned);
    entry.record.container_identity = Some(entry.record.parent_identity);
    entry.state = RetainedState::Ready;
    tx.save(&entry, RetainedState::Preparing).unwrap();
    tx.commit().unwrap();
    let mut tx = catalog.transaction().unwrap();
    entry.state = RetainedState::Purging;
    assert!(matches!(
        tx.save(&entry, RetainedState::Ready),
        Err(RetentionStoreError::Pinned(_))
    ));
    drop(tx);
    catalog
        .db
        .connection()
        .execute("UPDATE job SET state = 'done' WHERE id = 1", [])
        .unwrap();
    assert!(!catalog.get(entry.id).unwrap().pinned);
    let mut journal = crate::JobJournal::new(catalog.db).unwrap();
    assert_eq!(journal.prune(0).unwrap(), 0);
}

#[test]
fn verified_catalog_records_survive_salvage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.db");
    let (db, _) = Db::open(&path, false).unwrap();
    let mut source = RetentionCatalog::new(db).unwrap();
    let entry = insert(&mut source);
    drop(source);
    let fresh = Db::open_in_memory().unwrap();
    let report = crate::salvage_database(&path, &fresh).unwrap();
    assert!(report.retained.is_empty());
    assert_eq!(
        RetentionCatalog::new(fresh).unwrap().get(entry.id).unwrap(),
        entry
    );
}

#[test]
fn a_rejected_catalog_returns_manual_records_and_keeps_independent_user_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.db");
    let (db, _) = Db::open(&path, false).unwrap();
    db.connection()
        .execute(
            "INSERT INTO retained_entry (state, record) VALUES ('preparing', ?1)",
            [b"invalid record".as_slice()],
        )
        .unwrap();
    db.connection()
        .execute("INSERT INTO tags VALUES ('/sample', 'keep')", [])
        .unwrap();
    drop(db);
    let fresh = Db::open_in_memory().unwrap();
    let report = crate::salvage_database(&path, &fresh).unwrap();
    assert_eq!(report.retained.len(), 1);
    assert_eq!(
        report.retained[0].record.as_deref(),
        Some(b"invalid record".as_slice())
    );
    assert!(
        report
            .tables
            .iter()
            .any(|table| table.name == "retained_entry"
                && matches!(table.outcome, crate::SalvageOutcome::Reset { .. }))
    );
    assert_eq!(
        fresh
            .connection()
            .query_row("SELECT tag FROM tags", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    assert!(
        RetentionCatalog::new(fresh)
            .unwrap()
            .list_after(0, 10)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn an_untrusted_job_group_cannot_authorize_retention_purge_after_salvage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.db");
    let (db, _) = Db::open(&path, false).unwrap();
    db.connection()
        .execute(
            "INSERT INTO job VALUES (1, 'delete', 'running', 0, X'00')",
            [],
        )
        .unwrap();
    let mut source = RetentionCatalog::new(db).unwrap();
    let mut tx = source.transaction().unwrap();
    tx.insert(&record(), Some(crate::JobId(1))).unwrap();
    tx.commit().unwrap();
    drop(source);
    let fresh = Db::open_in_memory().unwrap();
    let report = crate::salvage_database(&path, &fresh).unwrap();
    assert_eq!(report.retained.len(), 1);
    assert!(
        report.tables.iter().any(|table| table.name == "job"
            && matches!(table.outcome, crate::SalvageOutcome::Reset { .. }))
    );
    assert!(
        RetentionCatalog::new(fresh)
            .unwrap()
            .list_after(0, 10)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn maintenance_inventory_paginates_only_live_validated_entries() {
    let mut catalog = catalog();
    let mut first = insert(&mut catalog);
    first.state = RetainedState::Aborted;
    let mut tx = catalog.transaction().unwrap();
    tx.save(&first, RetainedState::Preparing).unwrap();
    tx.commit().unwrap();
    let second = insert(&mut catalog);
    let third = insert(&mut catalog);
    let page = catalog.summaries_after(0, 1).unwrap();
    assert_eq!(page.len(), 1);
    assert_eq!(page[0].id(), second.id);
    assert_eq!(
        catalog.summaries_after(second.id.0, 1).unwrap()[0].id(),
        third.id
    );
    assert!(
        catalog
            .summaries_after(third.id.0, 1000)
            .unwrap()
            .is_empty()
    );
    assert!(catalog.summaries_after(0, 0).is_err());
}

#[test]
fn cleanup_projection_tracks_terminal_acknowledgement_and_refuses_active_entries() {
    let mut catalog = catalog();
    let mut entry = insert(&mut catalog);
    assert!(catalog.pending_cleanup(1, 100).unwrap().is_empty());
    assert!(catalog.acknowledge_cleanup(entry.id).is_err());
    entry.state = RetainedState::Aborted;
    let mut tx = catalog.transaction().unwrap();
    tx.save(&entry, RetainedState::Preparing).unwrap();
    tx.commit().unwrap();
    assert_eq!(catalog.pending_cleanup(1, 100).unwrap(), vec![entry.id]);
    catalog.begin_cleanup_attempt(entry.id).unwrap();
    catalog.acknowledge_cleanup(entry.id).unwrap();
    assert!(catalog.pending_cleanup(1, 100).unwrap().is_empty());
    assert_eq!(catalog.get(entry.id).unwrap().state, RetainedState::Aborted);
}

#[test]
fn terminal_cleanup_keeps_neighboring_u64_volumes_exact_above_sqlite_integer_range() {
    let mut catalog = catalog();
    let first_volume = 1_u64 << 63;
    let mut ids = Vec::new();
    for volume in [first_volume, first_volume + 1] {
        let mut record = record();
        record.parent_identity.volume = volume;
        record.nodes[0].identity.volume = volume;
        let mut tx = catalog.transaction().unwrap();
        let mut entry = tx.insert(&record, None).unwrap();
        entry.state = RetainedState::Aborted;
        tx.save(&entry, RetainedState::Preparing).unwrap();
        ids.push(entry.id);
        tx.commit().unwrap();
    }
    assert_eq!(
        catalog.pending_cleanup(first_volume + 1, 1).unwrap(),
        vec![ids[1]]
    );
    assert_eq!(
        catalog.pending_cleanup(first_volume, 1).unwrap(),
        vec![ids[0]]
    );
}

mod prune;
