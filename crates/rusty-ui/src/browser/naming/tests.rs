use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_core::{AppState, NameError};
use rusty_fs::{FakeFs, Provider, ProviderOps};

use super::NameProblem;
use crate::browser::Browser;

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
    while browser.loading() && Instant::now() < deadline {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
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
