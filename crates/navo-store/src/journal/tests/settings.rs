use super::*;

#[test]
fn new_sets_synchronous_full() {
    let journal = journal();
    let level: i64 = conn(&journal)
        .pragma_query_value(None, "synchronous", |row| row.get(0))
        .unwrap();
    assert_eq!(level, 2);
}

#[test]
fn new_turns_foreign_keys_on() {
    let journal = journal();
    let on: i64 = conn(&journal)
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .unwrap();
    assert_eq!(on, 1);
}

#[test]
fn unknown_state_text_is_reported() {
    let journal = journal();
    conn(&journal)
        .execute_batch(
            "INSERT INTO job (id, kind, state, created_at, plan) VALUES (7, 'copy', 'running', 1, x'00');
             INSERT INTO job_step (job_id, seq, state, detail) VALUES (7, 0, 'halfway', NULL);",
        )
        .unwrap();
    let error = journal.unfinished().unwrap_err();
    assert!(
        matches!(&error, JournalError::BadState(text) if text == "halfway"),
        "{error:?}"
    );
}
