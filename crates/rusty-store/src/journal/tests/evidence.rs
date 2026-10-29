use super::*;

#[test]
fn indexed_replacement_uses_exact_old_evidence_and_preserves_other_records() {
    let mut journal = journal();
    let job = journal
        .create_job("copy-retained-v1", b"plan", 2, 0)
        .unwrap();
    journal.record_held(job, b"overwrite", 1).unwrap();
    journal
        .replace_step_detail(job, &change(0, None, Some(b"intent")))
        .unwrap();
    journal
        .replace_step_detail(job, &change(1, None, Some(b"other")))
        .unwrap();
    journal
        .replace_step_detail(job, &change(0, Some(b"intent"), Some(b"created")))
        .unwrap();
    assert!(
        journal
            .replace_step_detail(job, &change(0, Some(b"intent"), None))
            .is_err()
    );
    assert_eq!(
        journal.step_detail(job, 0).unwrap(),
        Some(b"created".to_vec())
    );
    assert_eq!(
        journal.step_detail(job, 1).unwrap(),
        Some(b"other".to_vec())
    );
    assert_eq!(journal.held(job).unwrap(), [b"overwrite".to_vec()]);
}

#[test]
fn retirement_keeps_other_step_and_undo_evidence_and_never_changes_state() {
    let mut journal = journal();
    let job = journal
        .create_job("copy-retained-v1", b"plan", 2, 0)
        .unwrap();
    journal.record_held(job, b"overwrite", 1).unwrap();
    journal
        .mark_step(job, 0, StepState::Failed, Some(b"created"))
        .unwrap();
    journal
        .mark_step(job, 1, StepState::Done, Some(b"other"))
        .unwrap();
    journal
        .replace_step_detail(job, &change(0, Some(b"created"), None))
        .unwrap();
    assert!(journal.step_detail(job, 0).unwrap().is_none());
    assert_eq!(
        journal.step_detail(job, 1).unwrap(),
        Some(b"other".to_vec())
    );
    assert_eq!(journal.held(job).unwrap(), [b"overwrite".to_vec()]);
    let steps = step_rows(&journal, job);
    assert_eq!(steps[0].1, "failed");
    assert_eq!(steps[1].1, "done");
}

fn change<'a>(seq: u32, before: Option<&'a [u8]>, after: Option<&'a [u8]>) -> StepEvidence<'a> {
    StepEvidence { seq, before, after }
}
