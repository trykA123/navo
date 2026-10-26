use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_core::{AppState, NameError};
use rusty_fs::{FakeFs, Provider, ProviderOps};

use super::NameProblem;
use crate::browser::Browser;

#[test]
fn creation_lease_refuses_both_new_files_and_folders_before_mutation() {
    let fs = fake();
    let mut browser = browser(&fs);
    let mut operations = crate::browser::Operations::unlogged();
    operations.write = Some(Arc::new(|location| {
        Err(rusty_core::FsError {
            location: Some(location.clone()),
            ..rusty_core::FsError::new(rusty_core::FsErrorKind::AccessDenied)
        })
    }));
    browser.set_operations(operations);
    browser.new_file("blocked-file").unwrap();
    settle(&mut browser);
    browser.new_folder("blocked-folder").unwrap();
    settle(&mut browser);
    assert!(fs.metadata(&fs.location("home/blocked-file")).is_err());
    assert!(fs.metadata(&fs.location("home/blocked-folder")).is_err());
    assert!(browser.error(browser.state().active_tab().id()).is_some());
}

fn fake() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file("home/a.txt", 3);
    fs.add_file("home/b.txt", 4);
    Arc::new(fs)
}

fn browser(fs: &Arc<FakeFs>) -> Browser {
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let mut browser = Browser::new(
        AppState::new(fs.location("home")),
        provider,
        Arc::new(|| {}),
    );
    browser.start();
    settle(&mut browser);
    browser
}

fn settle(browser: &mut Browser) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while (browser.loading() || browser.running()) && Instant::now() < deadline {
        browser.poll();
        std::thread::yield_now();
    }
    browser.poll();
}

fn exists(fs: &FakeFs, path: &str) -> bool {
    fs.metadata(&fs.location(path)).is_ok()
}

fn cursor_name(browser: &Browser) -> Option<String> {
    let tab = browser.state().active_tab();
    tab.listing()
        .and_then(|l| l.get(tab.view().cursor))
        .map(|e| e.name.clone())
}

#[test]
fn rename_changes_the_name_and_follows_it() {
    let fs = fake();
    let mut browser = browser(&fs);
    assert_eq!(browser.rename("a.txt", "z.txt"), Ok(()));
    settle(&mut browser);
    assert!(exists(&fs, "home/z.txt"));
    assert!(!exists(&fs, "home/a.txt"));
    assert_eq!(cursor_name(&browser).as_deref(), Some("z.txt"));
}

#[test]
fn a_case_only_rename_goes_through() {
    let fs = fake();
    let mut browser = browser(&fs);
    assert_eq!(browser.rename("a.txt", "A.txt"), Ok(()));
    settle(&mut browser);
    assert!(exists(&fs, "home/A.txt"));
    assert!(!exists(&fs, "home/a.txt"));
}

#[test]
fn a_name_taken_by_another_entry_is_refused() {
    let fs = fake();
    let mut browser = browser(&fs);
    assert_eq!(browser.rename("a.txt", "B.TXT"), Err(NameProblem::Exists));
    assert_eq!(browser.new_folder("docs"), Err(NameProblem::Exists));
}

#[test]
fn an_invalid_name_is_refused_before_touching_the_disk() {
    let fs = fake();
    let mut browser = browser(&fs);
    assert_eq!(
        browser.rename("a.txt", "a?.txt"),
        Err(NameProblem::Invalid(NameError::Character('?')))
    );
    assert_eq!(
        browser.new_folder(""),
        Err(NameProblem::Invalid(NameError::Empty))
    );
    assert!(exists(&fs, "home/a.txt"));
}

#[test]
fn the_same_name_is_nothing_to_do() {
    let fs = fake();
    let mut browser = browser(&fs);
    assert_eq!(
        browser.rename("a.txt", "a.txt"),
        Err(NameProblem::Unchanged)
    );
}

#[test]
fn new_folder_creates_it_and_puts_the_cursor_on_it() {
    let fs = fake();
    let mut browser = browser(&fs);
    assert_eq!(browser.new_folder("reports"), Ok(()));
    settle(&mut browser);
    assert!(exists(&fs, "home/reports"));
    assert_eq!(cursor_name(&browser).as_deref(), Some("reports"));
}

#[test]
fn new_file_creates_an_empty_file_and_puts_the_cursor_on_it() {
    let fs = fake();
    let mut browser = browser(&fs);
    assert_eq!(browser.new_file("notes.txt"), Ok(()));
    settle(&mut browser);
    let created = fs.metadata(&fs.location("home/notes.txt")).map(|e| e.size);
    assert_eq!(created, Ok(0));
    assert_eq!(cursor_name(&browser).as_deref(), Some("notes.txt"));
}

#[test]
fn new_file_refuses_a_name_that_is_already_here() {
    let fs = fake();
    let mut browser = browser(&fs);
    assert_eq!(browser.new_file("A.TXT"), Err(NameProblem::Exists));
    assert_eq!(browser.new_file("docs"), Err(NameProblem::Exists));
    assert_eq!(
        fs.metadata(&fs.location("home/a.txt")).map(|e| e.size),
        Ok(3)
    );
}

#[test]
fn a_failed_rename_shows_its_error_in_the_pane() {
    use rusty_core::FsErrorKind;
    use rusty_fs::{FakeOp, Fault};
    let fs = fake();
    let mut browser = browser(&fs);
    fs.fault(
        "home/a.txt",
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    assert_eq!(browser.rename("a.txt", "z.txt"), Ok(()));
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert_eq!(
        browser.error(tab).map(|e| e.kind),
        Some(FsErrorKind::AccessDenied)
    );
    assert!(exists(&fs, "home/a.txt"));
}

#[test]
fn every_problem_reads_as_a_sentence() {
    assert_eq!(
        NameProblem::Exists.to_string(),
        "Something with this name is already here."
    );
    assert_eq!(
        NameProblem::Invalid(NameError::Empty).to_string(),
        "Type a name."
    );
}

fn names(browser: &Browser) -> Vec<String> {
    browser
        .state()
        .active_tab()
        .listing()
        .map(|l| l.entries().iter().map(|e| e.name.clone()).collect())
        .unwrap_or_default()
}

#[test]
fn a_new_name_outside_the_filter_drops_the_filter_and_is_shown() {
    let fs = fake();
    fs.add_file("home/apple.txt", 1);
    let mut browser = browser(&fs);
    browser.filter_push('a');
    assert_eq!(browser.rename("a.txt", "zebra.txt"), Ok(()));
    settle(&mut browser);
    let tab = browser.state().active_tab().id();
    assert_eq!(browser.filter_text(tab), None);
    assert_eq!(cursor_name(&browser).as_deref(), Some("zebra.txt"));
}

#[test]
fn a_new_name_inside_the_filter_keeps_it() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser.filter_push('a');
    assert_eq!(browser.rename("a.txt", "a2.txt"), Ok(()));
    settle(&mut browser);
    assert_eq!(names(&browser), ["a2.txt"]);
}

#[test]
fn a_name_hidden_by_the_filter_still_counts_as_taken() {
    let fs = fake();
    let mut browser = browser(&fs);
    browser.filter_push('a');
    assert_eq!(browser.rename("a.txt", "b.txt"), Err(NameProblem::Exists));
}

#[test]
fn a_case_only_rename_that_cannot_finish_puts_the_name_back() {
    use rusty_core::FsErrorKind;
    use rusty_fs::{FakeOp, Fault};
    let fs = fake();
    let mut browser = browser(&fs);
    let step = "home/.navo-rename-00000000-0";
    fs.fault(
        step,
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    assert_eq!(browser.rename("a.txt", "A.txt"), Ok(()));
    settle(&mut browser);
    assert!(exists(&fs, "home/a.txt"));
    assert!(!exists(&fs, step));
    let tab = browser.state().active_tab().id();
    assert!(browser.error(tab).is_some());
}

#[test]
fn a_stranded_case_step_is_listed_and_named_in_the_error() {
    use rusty_core::FsErrorKind;
    use rusty_fs::{FakeOp, Fault};
    let fs = fake();
    let mut browser = browser(&fs);
    let step = "home/.navo-rename-00000000-0";
    fs.fault(
        step,
        FakeOp::Rename,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    fs.fault(
        step,
        FakeOp::Rename,
        2,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    assert_eq!(browser.rename("a.txt", "A.txt"), Ok(()));
    settle(&mut browser);
    assert!(exists(&fs, step));
    assert!(names(&browser).contains(&".navo-rename-00000000-0".to_owned()));
    let tab = browser.state().active_tab().id();
    let location = browser.error(tab).and_then(|e| e.location.clone());
    assert_eq!(location, Some(fs.location(step)));
}

#[test]
fn the_same_name_closes_even_when_the_old_name_breaks_the_rules() {
    let fs = fake();
    fs.add_file("home/notes.", 1);
    let mut browser = browser(&fs);
    assert_eq!(
        browser.rename("notes.", "notes."),
        Err(NameProblem::Unchanged)
    );
}

#[test]
fn a_second_single_rename_waits_for_the_running_job() {
    let fs = fake();
    let mut browser = browser(&fs);
    assert_eq!(browser.rename("a.txt", "z.txt"), Ok(()));
    assert_eq!(browser.rename("b.txt", "y.txt"), Err(NameProblem::Busy));
    settle(&mut browser);
    assert!(exists(&fs, "home/z.txt"));
    assert!(exists(&fs, "home/b.txt"));
    assert!(!exists(&fs, "home/y.txt"));
}
