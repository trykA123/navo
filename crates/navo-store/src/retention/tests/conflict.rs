use super::*;

fn save(catalog: &mut RetentionCatalog, entry: &RetentionEntry, from: RetainedState) -> bool {
    let mut tx = catalog.transaction().unwrap();
    let saved = tx.save(entry, from).is_ok();
    tx.commit().unwrap();
    saved
}

fn conflicted_restore(catalog: &mut RetentionCatalog) -> RetentionEntry {
    let mut entry = insert(catalog);
    entry.record.container_identity = Some(entry.record.parent_identity);
    entry.state = RetainedState::Ready;
    assert!(save(catalog, &entry, RetainedState::Preparing));
    entry.state = RetainedState::Restoring;
    entry.target = Some(navo_core::retention::RestoreTarget {
        location: Location::new("/sample/a").unwrap(),
        parent_identity: entry.record.parent_identity,
    });
    assert!(save(catalog, &entry, RetainedState::Ready));
    entry.state = RetainedState::Conflict;
    assert!(save(catalog, &entry, RetainedState::Restoring));
    entry
}

#[test]
fn a_conflicted_restore_can_settle_to_ready_or_restored() {
    let mut catalog = catalog();
    let mut entry = conflicted_restore(&mut catalog);
    let mut landed = entry.clone();
    landed.state = RetainedState::Restored;
    assert!(save(&mut catalog, &landed, RetainedState::Conflict));
    let mut catalog = self::catalog();
    entry = conflicted_restore(&mut catalog);
    entry.state = RetainedState::Ready;
    entry.target = None;
    assert!(save(&mut catalog, &entry, RetainedState::Conflict));
    assert_eq!(catalog.get(entry.id).unwrap(), entry);
}

#[test]
fn a_conflict_without_a_restore_target_stays_conflicted() {
    let mut catalog = catalog();
    let mut entry = insert(&mut catalog);
    entry.state = RetainedState::Conflict;
    assert!(save(&mut catalog, &entry, RetainedState::Preparing));
    entry.record.container_identity = Some(entry.record.parent_identity);
    for state in [
        RetainedState::Ready,
        RetainedState::Restoring,
        RetainedState::Purging,
    ] {
        entry.state = state;
        assert!(
            !save(&mut catalog, &entry, RetainedState::Conflict),
            "{state:?}"
        );
    }
}

#[test]
fn a_conflicted_restore_never_moves_to_another_target_or_to_purging() {
    let mut catalog = catalog();
    let entry = conflicted_restore(&mut catalog);
    let mut moved = entry.clone();
    moved.state = RetainedState::Restored;
    moved.target.as_mut().unwrap().location = Location::new("/sample/b").unwrap();
    assert!(!save(&mut catalog, &moved, RetainedState::Conflict));
    for state in [RetainedState::Restoring, RetainedState::Purging] {
        let mut next = entry.clone();
        next.state = state;
        assert!(
            !save(&mut catalog, &next, RetainedState::Conflict),
            "{state:?}"
        );
    }
}
