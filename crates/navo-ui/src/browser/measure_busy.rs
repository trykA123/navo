//! A rebuilding volume's totals: wait off the UI thread, never walk, never invent a number.

use std::time::Duration;

use navo_core::Location;
use navo_fs::{CancelToken, FolderSize, Provider, folder_size};

use super::{FolderTotalsSource, TotalsState};

/// A rebuilding volume is asked again no sooner than this; cancellation ends the wait at once.
pub(super) const RETRY_EVERY: Duration = Duration::from_secs(1);

/// Asks the source until it answers. One job per asked folder keeps the wait inside the
/// shown-folder cap, and `Busy` comes back only for a cancelled job, so the row keeps its
/// pending indicator and shows no number.
pub(super) fn ask<S: FolderTotalsSource + ?Sized>(
    source: &S,
    folder: &Location,
    cancel: &CancelToken,
    every: Duration,
) -> TotalsState {
    loop {
        let state = source.query(folder);
        if state != TotalsState::Busy || cancel.wait_cancelled(every) {
            return state;
        }
    }
}

/// What a row shows for an answer: a complete index total, the provider's fallback, or nothing
/// at all while the volume rebuilds. Busy never reaches the provider.
pub(super) fn shown(
    state: TotalsState,
    provider: &dyn Provider,
    folder: &Location,
    cancel: &CancelToken,
) -> Option<FolderSize> {
    match state {
        TotalsState::Ready(totals) if totals.complete => {
            Some(FolderSize::apparent_only(totals.bytes))
        }
        TotalsState::Busy => None,
        _ => folder_size(provider, folder, cancel).ok(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    use navo_core::Command;
    use navo_fs::{FakeFs, FakeOp, FolderSize, VolumeKind};

    use super::*;
    use crate::browser::measure::folder_measure;
    use crate::browser::tests::{browser_at, fake, settle};
    use crate::browser::{
        Browser, FolderTotals, FolderTotalsQuery, FolderTotalsSource, no_folder_totals,
    };

    /// A rebuilding volume: the first `busy` asks answer Busy, then the real totals arrive.
    struct Rebuilding {
        busy: AtomicUsize,
        asks: AtomicUsize,
    }

    impl Rebuilding {
        fn new(busy: usize) -> Arc<Self> {
            Arc::new(Self {
                busy: AtomicUsize::new(busy),
                asks: AtomicUsize::new(0),
            })
        }

        fn asks(&self) -> usize {
            self.asks.load(Ordering::SeqCst)
        }
    }

    impl FolderTotalsSource for Rebuilding {
        fn query(&self, _: &Location) -> TotalsState {
            self.asks.fetch_add(1, Ordering::SeqCst);
            let left = self
                .busy
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                    Some(left.saturating_sub(1))
                })
                .unwrap_or(0);
            if left > 0 {
                return TotalsState::Busy;
            }
            TotalsState::Ready(FolderTotals {
                bytes: 42,
                files: 1,
                folders: 0,
                complete: true,
            })
        }
    }

    fn asked(source: &Rebuilding, at_least: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while source.asks() < at_least {
            assert!(
                Instant::now() < deadline,
                "the totals source was never asked"
            );
            std::thread::yield_now();
        }
    }

    fn busy_browser(busy: usize) -> (Browser, Arc<FakeFs>, Arc<Rebuilding>) {
        let fs = fake();
        let mut browser = browser_at(&fs, "home");
        let source = Rebuilding::new(busy);
        browser.set_folder_totals(source.clone());
        settle(&mut browser);
        (browser, fs, source)
    }

    #[test]
    fn a_busy_answer_never_reaches_the_provider() {
        let fs = fake();
        let size = shown(
            TotalsState::Busy,
            fs.as_ref(),
            &fs.location("home/docs"),
            &CancelToken::new(),
        );
        assert_eq!(size, None, "a rebuilding volume has no number to show");
        assert_eq!(
            fs.calls("home/docs", FakeOp::List),
            0,
            "Busy must not start a provider walk"
        );
    }

    #[test]
    fn a_cancelled_busy_wait_shows_no_number() {
        let fs = fake();
        let query: FolderTotalsQuery = Rebuilding::new(usize::MAX);
        let cancel = CancelToken::new();
        cancel.cancel();
        let size = folder_measure(&query, fs.as_ref(), &fs.location("home/docs"), &cancel);
        assert_eq!(size, None, "a cancelled wait has no number to show");
        assert_eq!(
            fs.calls("home/docs", FakeOp::List),
            0,
            "a cancelled wait must not start a provider walk"
        );
    }

    #[test]
    fn a_busy_ask_waits_a_second_and_then_shows_the_real_totals() {
        let fs = fake();
        let source = Rebuilding::new(1);
        let query: FolderTotalsQuery = source.clone();
        let started = Instant::now();
        let size = folder_measure(
            &query,
            fs.as_ref(),
            &fs.location("home/docs"),
            &CancelToken::new(),
        );
        assert_eq!(
            size,
            Some(FolderSize::apparent_only(42)),
            "the released totals must arrive"
        );
        assert!(
            started.elapsed() >= Duration::from_secs(1),
            "a rebuilding volume is asked again no sooner than a second"
        );
        assert_eq!(
            source.asks(),
            2,
            "one ask while busy, one after the release"
        );
        assert_eq!(
            fs.calls("home/docs", FakeOp::List),
            0,
            "waiting never walks"
        );
    }

    #[test]
    fn cancelling_ends_the_wait_without_another_ask() {
        let source = Rebuilding::new(usize::MAX);
        let query: FolderTotalsQuery = source.clone();
        let cancel = CancelToken::new();
        let (token, watcher) = (cancel.clone(), source.clone());
        std::thread::spawn(move || {
            while watcher.asks() == 0 {
                std::thread::yield_now();
            }
            token.cancel();
        });
        let folder = Location::new(r"C:\proj").unwrap();
        let started = Instant::now();
        let state = ask(query.as_ref(), &folder, &cancel, Duration::from_secs(5));
        assert_eq!(
            state,
            TotalsState::Busy,
            "a cancelled wait never falls back"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "cancellation must not wait out the interval"
        );
        assert_eq!(
            source.asks(),
            1,
            "cancellation ends the wait before the next ask"
        );
    }

    #[test]
    fn a_busy_row_keeps_its_pending_indicator_and_shows_no_number() {
        let (mut browser, fs, source) = busy_browser(1);
        browser.command(&Command::SelectionToggle).unwrap();
        asked(&source, 1);
        let tab = browser.state().active_tab().id();
        assert!(
            browser.measuring(),
            "the row keeps its pending indicator while the volume rebuilds"
        );
        assert_eq!(
            browser.details(tab)[0].size,
            0,
            "no number while totals are unknown"
        );
        settle(&mut browser);
        assert_eq!(
            browser.details(tab)[0].size,
            42,
            "the released totals arrive"
        );
        assert_eq!(fs.calls("home/docs", FakeOp::List), 0, "Busy must not walk");
        assert!(!browser.loading());
    }

    #[test]
    fn leaving_the_folder_cancels_a_busy_wait() {
        let (mut browser, fs, source) = busy_browser(usize::MAX);
        browser.command(&Command::SelectionToggle).unwrap();
        asked(&source, 1);
        browser
            .command(&Command::PaneNavigate {
                location: fs.location("home/music"),
            })
            .unwrap();
        settle(&mut browser);
        assert!(!browser.loading(), "the wait ends when the folder is left");
        assert_eq!(
            fs.calls("home/docs", FakeOp::List),
            0,
            "a cancelled wait never walks"
        );
    }

    #[test]
    fn a_busy_listing_waits_for_only_the_shown_folders() {
        let fs = FakeFs::new();
        for n in 0..600 {
            fs.add_file(&format!("many/f{n:04}/one.bin"), 1);
        }
        let fs = Arc::new(fs);
        let mut browser = browser_at(&fs, "many");
        settle(&mut browser);
        browser.set_volume_kinds(Arc::new(|_| VolumeKind::Ssd));
        let source = Rebuilding::new(1);
        browser.set_folder_totals(source.clone());
        browser.set_measures_shown_folders(true);
        settle(&mut browser);
        let tab = browser.state().active_tab().id();
        let sizes: Vec<u64> = browser.details(tab).iter().map(|row| row.size).collect();
        assert!(
            sizes[..500].iter().all(|&size| size == 42),
            "shown folders get totals"
        );
        assert!(
            sizes[500..].iter().all(|&size| size == 0),
            "past the cap nothing shows"
        );
        assert_eq!(
            source.asks(),
            501,
            "one ask per shown folder, one re-ask, no retry jobs"
        );
        assert_eq!(
            fs.calls("many/f0000", FakeOp::List),
            0,
            "no folder is walked"
        );
    }

    #[test]
    fn wait_through_trait_object_retries_after_a_second_and_returns_ready() {
        let source = Rebuilding::new(1);
        let query: FolderTotalsQuery = source.clone();
        let folder = Location::new(r"C:\proj").unwrap();
        let started = Instant::now();
        let state = query.wait(&folder, &CancelToken::new());
        assert_eq!(
            state,
            TotalsState::Ready(FolderTotals {
                bytes: 42,
                files: 1,
                folders: 0,
                complete: true,
            }),
            "waiting on a rebuilding trait object must retry until ready"
        );
        assert!(
            started.elapsed() >= Duration::from_secs(1),
            "trait-object wait must retry no sooner than a second"
        );
        assert_eq!(source.asks(), 2, "one ask while busy, one after release");
    }

    #[test]
    fn cancelling_trait_object_wait_returns_busy_without_another_ask() {
        let source = Rebuilding::new(usize::MAX);
        let query: FolderTotalsQuery = source.clone();
        let cancel = CancelToken::new();
        let (token, watcher) = (cancel.clone(), source.clone());
        let waiter = std::thread::spawn(move || {
            while watcher.asks() == 0 {
                std::thread::yield_now();
            }
            token.cancel();
        });
        let folder = Location::new(r"C:\proj").unwrap();
        let state = query.wait(&folder, &cancel);
        let _ = waiter.join();
        assert_eq!(
            state,
            TotalsState::Busy,
            "a cancelled wait returns Busy instead of falling back"
        );
        assert_eq!(
            source.asks(),
            1,
            "cancellation ends the wait before the next ask"
        );
    }

    #[test]
    fn unavailable_trait_object_wait_returns_unavailable() {
        let folder = Location::new(r"C:\proj").unwrap();
        assert_eq!(
            no_folder_totals().wait(&folder, &CancelToken::new()),
            TotalsState::Unavailable,
            "Unavailable sources return immediately without waiting"
        );
    }
}
