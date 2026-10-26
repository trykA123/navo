use std::sync::Arc;
use std::time::{Duration, Instant};

use rusty_core::{AppState, Location};
use rusty_fs::{LocalFs, ProviderOps};
use rusty_ui::browser::Browser;
use rusty_ui::file_list::SortBy;

use super::report::{self, Listing};

pub(super) const FILTER: &str = "sensor";
const DEADLINE: Duration = Duration::from_secs(600);
const SORTS: [(&str, SortBy); 4] = [
    ("size", SortBy::Size),
    ("date", SortBy::Date),
    ("extension", SortBy::Ext),
    ("name", SortBy::Name),
];

fn shown(browser: &Browser) -> Option<usize> {
    browser
        .state()
        .active_tab()
        .listing()
        .map(|listing| listing.entries().len())
}

fn timed(action: impl FnOnce()) -> f64 {
    let started = Instant::now();
    action();
    report::ms(started.elapsed())
}

pub(super) fn run(folder: &Location, label: &str) -> Result<Listing, String> {
    let provider: Arc<dyn ProviderOps> = Arc::new(LocalFs::new());
    let mut browser = Browser::new(AppState::new(folder.clone()), provider, Arc::new(|| {}));
    let started = Instant::now();
    browser.start();
    while browser.loading() && started.elapsed() < DEADLINE {
        browser.poll();
        std::thread::sleep(Duration::from_millis(1));
    }
    browser.poll();
    let list_ms = report::ms(started.elapsed());
    let entries = shown(&browser)
        .filter(|_| !browser.loading())
        .ok_or_else(|| format!("listing {label}: the folder did not load"))?;
    let sort_ms = SORTS
        .iter()
        .map(|&(name, by)| {
            (
                name.to_owned(),
                timed(|| {
                    browser.sort_by(by);
                }),
            )
        })
        .collect();
    let filter_keystroke_ms = FILTER
        .chars()
        .map(|ch| {
            timed(|| {
                browser.filter_push(ch);
            })
        })
        .collect();
    let filter_matches = shown(&browser).unwrap_or_default();
    browser.filter_clear();
    Ok(Listing {
        folder: label.to_owned(),
        entries,
        list_ms,
        sort_ms,
        filter: FILTER.to_owned(),
        filter_keystroke_ms,
        filter_matches,
    })
}
