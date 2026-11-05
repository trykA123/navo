use std::sync::Arc;
use std::time::{Duration, Instant};

use navo_core::AppState;
use navo_fs::view::Rows;
use navo_fs::{FakeFs, ProviderOps};

use super::tests::names;
use super::{SortBy, SortKey};
use crate::browser::Browser;
use crate::browser::cached::SYNC_ROWS;

const FILES: usize = SYNC_ROWS + 5_000;

fn big() -> Browser {
    let fs = FakeFs::new();
    fs.add_folder("home/docs");
    fs.add_file("home/ant.md", 1_000_000);
    for index in 0..FILES {
        fs.add_file(&format!("home/f{index:05}.txt"), ((index * 7) % 997) as u64);
    }
    let fs = Arc::new(fs);
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
    let deadline = Instant::now() + Duration::from_secs(10);
    while (browser.loading() || browser.running() || !browser.view_jobs.is_idle())
        && Instant::now() < deadline
    {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    browser.poll();
    assert!(browser.view_jobs.is_idle(), "a view job never answered");
}

fn row_names(rows: &Rows) -> Vec<String> {
    rows.iter().map(|e| e.name.to_text()).collect()
}

fn aligned(browser: &Browser) -> Vec<String> {
    let tab = browser.state().active_tab().id();
    let rows = row_names(&browser.rows(tab));
    assert_eq!(rows, names(browser), "rows and listing must stay aligned");
    rows
}

#[test]
fn an_async_sort_and_filter_equal_the_sync_path() {
    let mut browser = big();
    let tab = browser.state().active_tab().id();
    let source = browser.rows(tab).source().clone();
    browser.sort_by(SortBy::Size);
    settle(&mut browser);
    let key = SortKey {
        by: SortBy::Size,
        descending: false,
    };
    let sync = super::sorted_full(&source, key, true, &super::SortCount::default());
    assert_eq!(aligned(&browser), row_names(&sync.details));
    assert!(Arc::ptr_eq(browser.rows(tab).source(), &source));
    for ch in "f0001".chars() {
        assert!(browser.filter_push(ch));
    }
    settle(&mut browser);
    let want: Vec<String> = row_names(&sync.details)
        .into_iter()
        .filter(|name| name.contains("f0001"))
        .collect();
    assert_eq!(aligned(&browser), want);
    assert!(Arc::ptr_eq(browser.rows(tab).source(), &source));
}

#[test]
fn a_cancelled_sort_job_leaves_the_shown_view_untouched() {
    let mut browser = big();
    let tab = browser.state().active_tab().id();
    let before = browser.rows(tab);
    let shown = names(&browser);
    assert!(browser.sort_by(SortBy::Size));
    assert!(browser.view_jobs.cancel(tab).is_some());
    settle(&mut browser);
    assert!(Rows::same(&before, &browser.rows(tab)));
    assert_eq!(aligned(&browser), shown);
}

#[test]
fn sizes_measured_during_a_sort_job_survive() {
    let mut browser = big();
    let tab = browser.state().active_tab().id();
    browser.sort_by(SortBy::Size);
    assert!(!browser.view_jobs.is_idle());
    assert!(browser.show_size_of(tab, "docs", 4_242));
    settle(&mut browser);
    let rows = aligned(&browser);
    let at = rows.iter().position(|name| name == "docs").unwrap();
    assert_eq!(browser.rows(tab).get(at).map(|e| e.size), Some(4_242));
    browser.sort_by(SortBy::Size);
    settle(&mut browser);
    let rows = aligned(&browser);
    let at = rows.iter().position(|name| name == "docs").unwrap();
    assert_eq!(browser.rows(tab).get(at).map(|e| e.size), Some(4_242));
}
