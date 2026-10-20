use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::TestAppContext;
use rusty_core::AppState;

use super::{fake, open_options, options_for, press, settle, two_panes};
use crate::window_view::SearchHooks;
use crate::window_view::visits::Visits;

type Log = Arc<Mutex<Vec<(String, bool)>>>;

fn logging(options: &mut crate::window_view::StartOptions) -> Log {
    let log = Log::default();
    let sink = log.clone();
    options.search = SearchHooks {
        record_visit: Arc::new(move |location, is_folder| {
            sink.lock().unwrap().push((location.to_owned(), is_folder));
        }),
        ..SearchHooks::none()
    };
    log
}

#[gpui_kit::test]
fn a_loaded_folder_is_recorded_once_within_two_seconds(cx: &mut TestAppContext) {
    let fs = fake();
    let mut options = options_for(fs.clone(), two_panes(&fs));
    let log = logging(&mut options);
    let (_, view) = open_options(cx, options);
    settle(cx, &view);
    let home = fs.location("home");
    let seen = log.lock().unwrap().clone();
    assert_eq!(seen, [(home.as_str().to_owned(), true)]);
}

#[gpui_kit::test]
fn a_failed_listing_is_not_recorded(cx: &mut TestAppContext) {
    let fs = fake();
    let mut options = options_for(fs.clone(), AppState::new(fs.location("missing")));
    let log = logging(&mut options);
    let (_, view) = open_options(cx, options);
    settle(cx, &view);
    assert!(log.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn enter_on_a_file_records_the_file(cx: &mut TestAppContext) {
    let fs = fake();
    let mut options = options_for(fs.clone(), two_panes(&fs));
    let log = logging(&mut options);
    let (window, view) = open_options(cx, options);
    press(cx, window, &view, "down");
    press(cx, window, &view, "enter");
    let file = fs.location("home/a.txt");
    let seen = log.lock().unwrap().clone();
    assert!(
        seen.contains(&(file.as_str().to_owned(), false)),
        "{seen:?}"
    );
}

#[test]
fn a_folder_counts_once_per_two_seconds() {
    let start = Instant::now();
    let mut visits = Visits::default();
    assert!(visits.is_due("a", start));
    assert!(!visits.is_due("a", start + Duration::from_millis(1_900)));
    assert!(visits.is_due("b", start + Duration::from_millis(1_900)));
    assert!(visits.is_due("a", start + Duration::from_millis(2_000)));
}
