use super::*;

fn attempts(catalog: &RetentionCatalog) -> Vec<u32> {
    catalog
        .summaries_after(0, 1000)
        .unwrap()
        .iter()
        .map(|summary| summary.attempts())
        .collect()
}

#[test]
fn a_maintenance_attempt_is_counted_on_the_live_entry_and_kept_across_saves() {
    let mut catalog = catalog();
    let mut first = insert(&mut catalog);
    insert(&mut catalog);
    assert_eq!(attempts(&catalog), [0, 0]);
    catalog.begin_maintenance_attempt(first.id).unwrap();
    catalog.begin_maintenance_attempt(first.id).unwrap();
    assert_eq!(attempts(&catalog), [2, 0]);
    first.state = RetainedState::Conflict;
    let mut tx = catalog.transaction().unwrap();
    tx.save(&first, RetainedState::Preparing).unwrap();
    tx.commit().unwrap();
    assert_eq!(attempts(&catalog), [2, 0]);
    assert_eq!(catalog.get(first.id).unwrap(), first);
}

#[test]
fn a_maintenance_attempt_on_a_finished_or_missing_entry_is_refused() {
    let mut catalog = catalog();
    let mut entry = insert(&mut catalog);
    entry.state = RetainedState::Aborted;
    let mut tx = catalog.transaction().unwrap();
    tx.save(&entry, RetainedState::Preparing).unwrap();
    tx.commit().unwrap();
    assert!(catalog.begin_maintenance_attempt(entry.id).is_err());
    assert!(catalog.begin_maintenance_attempt(RetentionId(99)).is_err());
}

#[test]
fn a_cancelled_maintenance_attempt_is_taken_back_and_never_goes_below_zero() {
    let mut catalog = catalog();
    let entry = insert(&mut catalog);
    catalog.begin_maintenance_attempt(entry.id).unwrap();
    catalog.cancel_maintenance_attempt(entry.id).unwrap();
    assert_eq!(attempts(&catalog), [0]);
    catalog.cancel_maintenance_attempt(entry.id).unwrap();
    assert_eq!(attempts(&catalog), [0]);
}
