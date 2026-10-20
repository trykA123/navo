use super::*;

fn link_plan(fs: &FakeFs) -> OperationPlan {
    plan(&request(fs, OpKind::Link, &["src/a.txt"], "dst"))
}

fn unsupported(outcome: JobOutcome) {
    let JobOutcome::Failed { seq, error } = outcome else {
        panic!("link execution must fail: {outcome:?}");
    };
    assert_eq!(seq, 0);
    assert_eq!(error.kind, FsErrorKind::NotSupportedYet);
    assert!(error.message().contains("not supported yet"));
}

#[test]
fn executing_a_link_fails_without_changing_files() {
    for workers in MODES {
        let fs = fixture(&[("src/a.txt", 8)]);
        let before = tree_of(&fs, "");
        let (outcome, harness) = run_in(workers, &fs, &link_plan(&fs));
        unsupported(outcome);
        assert_eq!(harness.logged(), [(0, StepOutcome::Failed)]);
        assert_eq!(tree_of(&fs, ""), before);
    }
}

#[test]
fn undoing_a_link_fails_without_changing_files() {
    let fs = fixture(&[("src/a.txt", 8)]);
    add_link(&fs, "dst", "a.txt");
    let before = tree_of(&fs, "");
    let outcome = Harness::default().roll_back(&fs, &link_plan(&fs), &[0]);
    unsupported(outcome);
    assert_eq!(tree_of(&fs, ""), before);
}
