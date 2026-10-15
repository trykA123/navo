use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_core::{AppState, Command, OpKind};
use rusty_fs::{FakeFs, ProviderOps};

use crate::browser::Browser;

fn fake() -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_folder("home/prototype");
    fs.add_folder("home/src");
    fs.add_file("home/src/main.rs", 4);
    fs.add_file("home/app-icon.svg", 5);
    fs.add_file("home/package-lock.json", 6);
    fs.add_file("home/package.json", 7);
    Arc::new(fs)
}

fn browser(fs: &Arc<FakeFs>) -> Browser {
    let provider: Arc<dyn ProviderOps> = fs.clone();
    let state = AppState::new(fs.location("home"));
    let mut browser = Browser::new(state, provider, Arc::new(|| {}));
    browser.start();
    settle(&mut browser);
    browser
}

fn settle(browser: &mut Browser) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while (browser.loading() || browser.running()) && Instant::now() < deadline {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    browser.poll();
}

fn names(browser: &Browser) -> Vec<String> {
    browser
        .state()
        .active_tab()
        .listing()
        .map(|l| l.entries().iter().map(|e| e.name.clone()).collect())
        .unwrap_or_default()
}

fn detail_names(browser: &Browser) -> Vec<String> {
    let tab = browser.state().active_tab().id();
    browser
        .details(tab)
        .iter()
        .map(|e| e.name.to_text())
        .collect()
}

fn typed(browser: &mut Browser, text: &str) {
    for ch in text.chars() {
        browser.filter_push(ch);
    }
}

fn filter_text(browser: &Browser) -> Option<String> {
    let tab = browser.state().active_tab().id();
    browser.filter_text(tab).map(str::to_owned)
}

#[test]
fn typing_keeps_only_names_that_start_with_the_text() {
    let fs = fake();
    let mut browser = browser(&fs);
    typed(&mut browser, "p");
    assert_eq!(
        names(&browser),
        ["prototype", "package-lock.json", "package.json"]
    );
    typed(&mut browser, "A");
    assert_eq!(names(&browser), ["package-lock.json", "package.json"]);
    assert_eq!(filter_text(&browser).as_deref(), Some("pA"));
}

#[test]
fn the_details_follow_the_filtered_rows() {
    let fs = fake();
    let mut browser = browser(&fs);
    typed(&mut browser, "pa");
    assert_eq!(names(&browser), detail_names(&browser));
}

#[test]
fn a_star_matches_anywhere_in_the_name() {
    let fs = fake();
    let mut browser = browser(&fs);
    typed(&mut browser, "*icon");
    assert_eq!(names(&browser), ["app-icon.svg"]);
}

#[test]
fn a_letter_that_matches_nothing_is_refused() {
    let fs = fake();
    let mut browser = browser(&fs);
    typed(&mut browser, "pa");
    assert!(!browser.filter_push('z'));
    assert_eq!(names(&browser), ["package-lock.json", "package.json"]);
    assert_eq!(filter_text(&browser).as_deref(), Some("pa"));
}

#[test]
fn a_space_or_control_character_is_not_typed() {
    let fs = fake();
    let mut browser = browser(&fs);
    assert!(!browser.filter_push(' '));
    assert!(!browser.filter_push('\u{8}'));
    assert_eq!(filter_text(&browser), None);
}

#[test]
fn backspace_widens_and_clear_shows_everything() {
    let fs = fake();
    let mut browser = browser(&fs);
    typed(&mut browser, "pa");
    assert!(browser.filter_pop());
    assert_eq!(names(&browser).len(), 3);
    assert!(browser.filter_pop());
    assert_eq!(names(&browser).len(), 5);
    assert_eq!(filter_text(&browser), None);
    assert!(!browser.filter_pop());
    typed(&mut browser, "s");
    assert!(browser.filter_clear());
    assert_eq!(names(&browser).len(), 5);
    assert!(!browser.filter_clear());
}

#[test]
fn leaving_the_folder_drops_the_filter() {
    let fs = fake();
    let mut browser = browser(&fs);
    typed(&mut browser, "s");
    browser
        .command(&Command::PaneNavigate {
            location: fs.location("home/src"),
        })
        .unwrap();
    assert_eq!(filter_text(&browser), None);
    settle(&mut browser);
    assert_eq!(names(&browser), ["main.rs"]);
    browser.command(&Command::PaneBack).unwrap();
    settle(&mut browser);
    assert_eq!(names(&browser).len(), 5);
}

#[test]
fn a_refresh_keeps_the_filter() {
    let fs = fake();
    let mut browser = browser(&fs);
    typed(&mut browser, "pa");
    fs.add_file("home/pack.txt", 1);
    browser.command(&Command::PaneRefresh).unwrap();
    settle(&mut browser);
    assert_eq!(
        names(&browser),
        ["pack.txt", "package-lock.json", "package.json"]
    );
}

#[test]
fn operations_act_on_the_shown_rows() {
    let fs = fake();
    let mut browser = browser(&fs);
    typed(&mut browser, "pa");
    browser.prepare(OpKind::Recycle);
    settle(&mut browser);
    let sources = browser.pending().map(|p| p.sources.clone());
    assert_eq!(sources, Some(vec![fs.location("home/package-lock.json")]));
}

#[test]
fn a_refresh_that_matches_nothing_drops_the_filter() {
    let fs = fake();
    let mut browser = browser(&fs);
    typed(&mut browser, "pa");
    fs.remove("home/package.json");
    fs.remove("home/package-lock.json");
    browser.command(&Command::PaneRefresh).unwrap();
    settle(&mut browser);
    assert_eq!(filter_text(&browser), None);
    assert_eq!(names(&browser), ["prototype", "src", "app-icon.svg"]);
}

#[test]
fn a_folder_that_fails_to_load_still_drops_the_filter() {
    use rusty_core::FsErrorKind;
    use rusty_fs::{FakeOp, Fault};
    let fs = fake();
    let mut browser = browser(&fs);
    typed(&mut browser, "s");
    fs.fault(
        "home/src",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    browser
        .command(&Command::PaneNavigate {
            location: fs.location("home/src"),
        })
        .unwrap();
    settle(&mut browser);
    browser.command(&Command::PaneBack).unwrap();
    settle(&mut browser);
    assert_eq!(filter_text(&browser), None);
    assert_eq!(names(&browser).len(), 5);
}

#[test]
fn a_size_that_arrives_while_its_folder_is_hidden_is_kept() {
    let fs = fake();
    fs.add_file("home/prototype/big.bin", 9_000);
    let mut browser = browser(&fs);
    browser.command(&Command::SelectionToggle).unwrap();
    typed(&mut browser, "pa");
    settle(&mut browser);
    browser.filter_clear();
    let tab = browser.state().active_tab().id();
    let prototype = browser
        .details(tab)
        .iter()
        .find(|e| e.name.to_text() == "prototype")
        .map(|e| e.size);
    assert_eq!(prototype, Some(9_000));
}
