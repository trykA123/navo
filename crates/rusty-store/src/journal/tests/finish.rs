use super::*;

#[test]
fn unfinished_lists_only_running_jobs_oldest_first_with_ordered_steps() {
    let mut journal = journal();
    let newer = journal.create_job("move", b"newer", 2, 300).unwrap();
    let done = journal.create_job("copy", b"done", 1, 50).unwrap();
    let older = journal.create_job("copy", b"older", 3, 100).unwrap();
    let mut finished = Vec::new();
    for (state, created_at) in [
        (JobState::Failed, 10),
        (JobState::Cancelled, 20),
        (JobState::RolledBack, 30),
    ] {
        finished.push((
            journal.create_job("delete", b"x", 1, created_at).unwrap(),
            state,
        ));
    }
    journal.finish(done, JobState::Done).unwrap();
    for (job, state) in finished {
        journal.finish(job, state).unwrap();
    }
    journal
        .mark_step(older, 1, StepState::Done, Some(b"d"))
        .unwrap();

    let expected = vec![
        UnfinishedJob {
            id: older,
            kind: "copy".to_owned(),
            plan: b"older".to_vec(),
            created_at: 100,
            steps: vec![
                pending(0),
                JournalStep {
                    seq: 1,
                    state: StepState::Done,
                    detail: Some(b"d".to_vec()),
                },
                pending(2),
            ],
        },
        UnfinishedJob {
            id: newer,
            kind: "move".to_owned(),
            plan: b"newer".to_vec(),
            created_at: 300,
            steps: vec![pending(0), pending(1)],
        },
    ];
    assert_eq!(journal.unfinished().unwrap(), expected);
}
