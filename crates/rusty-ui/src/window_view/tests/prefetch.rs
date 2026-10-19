use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use rusty_cache::{CacheKeys, ListingCache};
use rusty_core::AppState;
use rusty_fs::{FakeFs, FakeOp};

use super::super::FerriteWindow;
use super::{open_options, options_for, settle};

struct Rig {
    window: AnyWindowHandle,
    view: Entity<FerriteWindow>,
    fs: Arc<FakeFs>,
    cache: ListingCache,
}

impl Rig {
    fn open(cx: &mut TestAppContext) -> Self {
        let fs = FakeFs::new();
        fs.add_file("home/alpha/one.txt", 1);
        fs.add_file("home/beta/x.txt", 1);
        fs.add_file("home/beta/y.txt", 1);
        fs.add_file("home/notes.txt", 1);
        let fs = Arc::new(fs);
        let cache = ListingCache::default();
        let options = super::super::StartOptions {
            listing_cache: cache.clone(),
            ..options_for(fs.clone(), AppState::new(fs.location("home")))
        };
        let (window, view) = open_options(cx, options);
        Self {
            window,
            view,
            fs,
            cache,
        }
    }

    fn press(&self, cx: &mut TestAppContext, key: &str) {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            window.press(key, cx);
        })
        .expect("window");
        cx.run_until_parked();
    }

    fn wait(cx: &mut TestAppContext, millis: u64) {
        cx.executor().advance_clock(Duration::from_millis(millis));
        cx.run_until_parked();
    }

    fn finish_prefetches(&self, cx: &mut TestAppContext) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            self.view.update(cx, |view, cx| view.wake(cx));
            cx.run_until_parked();
            let busy = self
                .view
                .read_with(cx, |view, _| view.browser.prefetching() > 0);
            if !busy || Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn lists(&self, path: &str) -> u32 {
        self.fs.calls(path, FakeOp::List)
    }

    fn is_cached(&self, path: &str) -> bool {
        let key = CacheKeys::default().key(&self.fs.location(path)).unwrap();
        self.cache.get(&key).is_some()
    }
}

#[gpui_kit::test]
fn the_cursor_resting_150_ms_on_a_folder_prefetches_it(cx: &mut TestAppContext) {
    let rig = Rig::open(cx);
    rig.press(cx, "down");
    Rig::wait(cx, 140);
    rig.finish_prefetches(cx);
    assert_eq!(rig.lists("home/beta"), 0);
    Rig::wait(cx, 10);
    rig.finish_prefetches(cx);
    assert_eq!(rig.lists("home/beta"), 1);
    assert!(rig.is_cached("home/beta"));
}

#[gpui_kit::test]
fn moving_on_after_100_ms_prefetches_nothing(cx: &mut TestAppContext) {
    let rig = Rig::open(cx);
    rig.press(cx, "down");
    Rig::wait(cx, 100);
    rig.press(cx, "down");
    Rig::wait(cx, 500);
    rig.finish_prefetches(cx);
    assert_eq!(rig.lists("home/alpha"), 0);
    assert_eq!(rig.lists("home/beta"), 0);
    assert!(!rig.is_cached("home/beta"));
}

#[gpui_kit::test]
fn enter_after_a_rest_shows_the_prefetched_rows_at_once(cx: &mut TestAppContext) {
    let rig = Rig::open(cx);
    rig.press(cx, "down");
    Rig::wait(cx, 150);
    rig.finish_prefetches(cx);
    rig.press(cx, "enter");
    let (names, is_cached) = rig.view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab();
        let names: Vec<String> = tab
            .listing()
            .map(|l| l.entries().iter().map(|e| e.name.clone()).collect())
            .unwrap_or_default();
        (names, view.browser.is_cached(tab.id()))
    });
    assert_eq!(names, ["x.txt", "y.txt"]);
    assert!(is_cached);
    settle(cx, &rig.view);
}
