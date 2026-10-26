use super::*;

fn logged(browser: &mut Browser, calls: &Arc<Mutex<Vec<OperationPlan>>>, fail: bool) {
    let calls = calls.clone();
    let mut hooks = browser.rename_hooks().unwrap().clone();
    hooks.log = Some(Arc::new(move |plan, _status| {
        calls.lock().unwrap().push(plan.clone());
        if fail {
            Err("log disk full".into())
        } else {
            Ok(())
        }
    }));
    browser.set_rename_hooks(Some(hooks));
}

#[test]
fn enabled_rename_logging_records_one_completed_batch() {
    let fs = home(&["a", "b"]);
    let mut browser = open(&fs, &Journal::new());
    let calls = Arc::new(Mutex::new(Vec::new()));
    logged(&mut browser, &calls, false);
    browser.set_rename_logging(true);
    let plan = batch(&fs, &[("a", "b"), ("b", "a")]);
    run(&mut browser, plan.clone(), None);
    assert_eq!(*calls.lock().unwrap(), [plan]);
    assert_eq!(last_job(&browser).status, JobStatus::Done);
}

#[test]
fn rename_logging_is_disabled_by_default() {
    let fs = home(&["a"]);
    let mut browser = open(&fs, &Journal::new());
    let calls = Arc::new(Mutex::new(Vec::new()));
    logged(&mut browser, &calls, false);
    run(&mut browser, batch(&fs, &[("a", "b")]), None);
    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn failed_renames_and_undo_do_not_append_log_entries() {
    let fs = home(&["a"]);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    let calls = Arc::new(Mutex::new(Vec::new()));
    logged(&mut browser, &calls, false);
    browser.set_rename_logging(true);
    fs.fault(
        "home/a",
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    run(&mut browser, batch(&fs, &[("a", "b")]), None);
    assert!(calls.lock().unwrap().is_empty());
    assert!(matches!(last_job(&browser).status, JobStatus::Failed(_)));
    run(&mut browser, batch(&fs, &[("a", "b")]), None);
    assert_eq!(calls.lock().unwrap().len(), 1);
    undo(&mut browser, &journal);
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[test]
fn a_log_failure_keeps_the_rename_and_undo_and_reports_the_problem() {
    let fs = home(&["a"]);
    let journal = Journal::new();
    let mut browser = open(&fs, &journal);
    logged(&mut browser, &Arc::new(Mutex::new(Vec::new())), true);
    browser.set_rename_logging(true);
    run(&mut browser, batch(&fs, &[("a", "b")]), None);
    assert_eq!(last_job(&browser).status, JobStatus::Done);
    assert_eq!(
        last_job(&browser).report.log_error.as_deref(),
        Some("log disk full")
    );
    assert_eq!(size_of(&fs, "b"), Some(10));
    assert!(browser.undo_rename(journal.offer().unwrap()));
    settle(&mut browser);
    assert_eq!(size_of(&fs, "a"), Some(10));
}

#[test]
fn an_incomplete_rename_logs_only_applied_pairs_and_marks_them_incomplete() {
    let fs = home(&NAMES);
    let mut browser = open(&fs, &Journal::new());
    let calls = Arc::new(Mutex::new(Vec::new()));
    let capture = calls.clone();
    let mut hooks = browser.rename_hooks().unwrap().clone();
    hooks.log = Some(Arc::new(move |plan, status| {
        capture.lock().unwrap().push((plan.steps.clone(), status));
        Ok(())
    }));
    browser.set_rename_hooks(Some(hooks));
    browser.set_rename_logging(true);
    let plan = batch(&fs, &SIMPLE);
    fs.fault(
        &source_of(&plan, 2),
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    fs.fault(
        &target_of(&plan, 1),
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::Locked),
    );
    run(&mut browser, plan.clone(), None);
    let calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].0, plan.steps[..2]);
    assert_eq!(calls[0].1, crate::browser::RenameLogStatus::Incomplete);
}
