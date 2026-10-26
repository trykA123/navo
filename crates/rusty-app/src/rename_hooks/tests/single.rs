use std::time::{Duration, Instant};

use rusty_core::AppState;
use rusty_fs::{Provider, ProviderOps};
use rusty_ui::browser::{Browser, Operations};

use super::*;

fn settle(browser: &mut Browser) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while (browser.loading() || browser.running()) && Instant::now() < deadline {
        browser.poll();
        std::thread::yield_now();
    }
    browser.poll();
    assert!(!browser.loading() && !browser.running());
}

fn browser(fs: &Arc<FakeFs>, journal: SharedJournal) -> Browser {
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let mut browser = Browser::new(AppState::new(fs.location("d")), provider, Arc::new(|| {}));
    browser.set_operations(Operations {
        retain_deletes: None,
        write: None,
        log: log_factory(journal.clone()),
        protected: Vec::new(),
    });
    browser.set_rename_hooks(Some(hooks(Some(journal), None)));
    browser.start();
    settle(&mut browser);
    browser
}

#[test]
fn a_single_rename_is_journaled_and_can_be_undone_after_reopening() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("ferrite.db");
    let fs = Arc::new(FakeFs::new());
    fs.add_file("d/a", 5);
    let mut first = browser(&fs, jobs::open(&path).unwrap());
    first.rename("a", "b").unwrap();
    settle(&mut first);
    assert!(fs.metadata(&fs.location("d/b")).is_ok());
    drop(first);
    let mut reopened = browser(&fs, jobs::open(&path).unwrap());
    let offer = (reopened.rename_hooks().unwrap().last)()
        .unwrap()
        .expect("journaled rename");
    assert_eq!(offer.items(), 1);
    assert!(reopened.undo_rename(offer));
    settle(&mut reopened);
    assert!(fs.metadata(&fs.location("d/a")).is_ok());
    assert!(fs.metadata(&fs.location("d/b")).is_err());
}

#[test]
fn a_single_folder_rename_and_its_undo_keep_the_contents() {
    let fs = Arc::new(FakeFs::new());
    fs.add_file("d/old/child", 5);
    let mut browser = browser(&fs, memory_journal());
    browser.rename("old", "new").unwrap();
    settle(&mut browser);
    let offer = (browser.rename_hooks().unwrap().last)()
        .unwrap()
        .expect("journaled folder rename");
    assert!(browser.undo_rename(offer));
    settle(&mut browser);
    assert_eq!(fs.metadata(&fs.location("d/old/child")).unwrap().size, 5);
    assert!(fs.metadata(&fs.location("d/new")).is_err());
}

#[test]
fn a_case_only_single_rename_is_undoable_on_an_insensitive_provider() {
    let fs = Arc::new(FakeFs::new().case_insensitive());
    fs.add_file("d/a", 5);
    let mut browser = browser(&fs, memory_journal());
    browser.rename("a", "A").unwrap();
    settle(&mut browser);
    let offer = (browser.rename_hooks().unwrap().last)()
        .unwrap()
        .expect("journaled case rename");
    assert_eq!(
        fs.metadata(&fs.location("d/A")).unwrap().name.to_text(),
        "A"
    );
    assert!(browser.undo_rename(offer));
    settle(&mut browser);
    assert_eq!(
        fs.metadata(&fs.location("d/a")).unwrap().name.to_text(),
        "a"
    );
}
