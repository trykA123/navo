//! Measure Browser operations and provider batch timings for bench reports.

use std::ops::ControlFlow;
use std::sync::Arc;
use std::time::{Duration, Instant};

use navo_core::{AppState, Location};
use navo_fs::{CancelToken, EntryBatch, LocalFs, Provider, ProviderOps};
use navo_ui::browser::Browser;
use navo_ui::file_list::SortBy;

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

/// Measure folder operations in milliseconds, failing if either listing pass fails.
pub(super) fn run(folder: &Location, label: &str, filter: &str) -> Result<Listing, String> {
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
    let filter_keystroke_ms = filter
        .chars()
        .map(|ch| {
            timed(|| {
                browser.filter_push(ch);
            })
        })
        .collect();
    let filter_matches = shown(&browser).unwrap_or_default();
    browser.filter_clear();
    let (first_batch_ms, provider_ms) = provider_times(folder, label)?;
    Ok(Listing {
        folder: label.to_owned(),
        entries,
        list_ms,
        first_batch_ms,
        provider_ms,
        sort_ms,
        filter: filter.to_owned(),
        filter_keystroke_ms,
        filter_matches,
    })
}

/// Measure first non-empty batch and total milliseconds, or return the provider error.
fn provider_times(folder: &Location, label: &str) -> Result<(f64, f64), String> {
    let started = Instant::now();
    let mut first_batch_ms = None;
    let mut sink = |batch: EntryBatch| {
        if first_batch_ms.is_none() && !batch.entries.is_empty() {
            first_batch_ms = Some(report::ms(started.elapsed()));
        }
        ControlFlow::Continue(())
    };
    LocalFs::new()
        .list(folder, &mut sink, &CancelToken::new())
        .map_err(|error| format!("provider listing {label}: {error:?}"))?;
    let provider_ms = report::ms(started.elapsed());
    Ok((first_batch_ms.unwrap_or_default(), provider_ms))
}
