//! App soak (AM5): one headless window with two panes over a large fake tree runs the same key
//! loop many times and checks that the window's own counters return to their baseline.

use std::sync::Arc;
use std::time::Instant;

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext};
use navo_fs::{Attributes, EntryKind, FakeFs, FsEntry};

use super::{hello_head, open_options, options_for, press, two_panes, viewer_open};
use crate::window_view::{FileHooks, FileText, NavoWindow, StartOptions};

/// Rounds between two counter samples.
const SAMPLE_EVERY: usize = 100;
/// Child folders under each top folder of the fake tree.
const FAN_OUT: usize = 250;

type Sample = Vec<(&'static str, usize)>;

/// Reads a positive number from the environment variable `name`, or `default` when unset.
fn tunable(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(default)
}

/// One folder entry named `name` for a bulk insert.
fn folder(name: &str) -> FsEntry {
    FsEntry {
        name: navo_core::ProviderPath::from_text(name),
        kind: EntryKind::Folder,
        size: 0,
        modified_ms: None,
        attributes: Attributes::default(),
    }
}

/// Builds `home` with `folders` folders in total: top folders of `FAN_OUT` children each, and a
/// text file in each top folder for the F3 viewer.
fn big_tree(folders: usize) -> Arc<FakeFs> {
    let fs = FakeFs::new();
    let tops = folders.div_ceil(FAN_OUT).max(1);
    fs.add_folder("home");
    fs.add_entries("home", (0..tops).map(|i| folder(&format!("d{i:05}"))));
    for top in 0..tops {
        let children = (0..FAN_OUT).map(|i| folder(&format!("s{i:03}")));
        fs.add_entries(&format!("home/d{top:05}"), children);
        fs.add_file(&format!("home/d{top:05}/note.txt"), 6);
    }
    Arc::new(fs)
}

/// Opens the soak window over `fs` with a viewer that reads every file as "hello".
fn soak_window(
    cx: &mut TestAppContext,
    fs: Arc<FakeFs>,
    cache: navo_cache::ListingCache,
) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let state = two_panes(&fs);
    let options = StartOptions {
        files: FileHooks {
            read: Arc::new(|_| Ok(FileText::plain("hello\n"))),
            head: Arc::new(|_| Ok(hello_head())),
            ..FileHooks::none()
        },
        listing_cache: cache,
        ..options_for(fs, state)
    };
    open_options(cx, options)
}

/// The window's counters: tabs, focus handles, cached pane views, the viewer, the browser's
/// per-tab maps and the listing cache's entries.
fn sample(
    cx: &mut TestAppContext,
    view: &Entity<NavoWindow>,
    cache: &navo_cache::ListingCache,
) -> Sample {
    let stats = cache.stats();
    view.read_with(cx, |view, _| {
        let state = view.browser.state();
        let tabs = view
            .panes()
            .iter()
            .filter_map(|pane| state.pane(*pane))
            .map(|pane| pane.tabs().len())
            .sum();
        let mut counts = vec![
            ("tabs", tabs),
            ("focus", view.focus.len()),
            ("cached_panes", view.cached.pane_ids().count()),
            ("viewer", usize::from(view.viewer.is_some())),
            ("cache_entries", stats.entries),
            ("cache_pinned", stats.pinned_entries),
        ];
        counts.extend(view.browser.soak_counts());
        counts
    })
}

/// The active tab's folder.
fn here(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> navo_core::Location {
    view.read_with(cx, |view, _| {
        view.browser.state().active_tab().location().clone()
    })
}

/// One soak round: enter a folder, view its file with F3 and close it, open the parent, then
/// open and close a tab. Fails when a step does not do what it should.
fn round(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<NavoWindow>) {
    let home = here(cx, view);
    press(cx, window, view, "enter");
    assert_ne!(here(cx, view), home, "enter did not open the folder");
    press(cx, window, view, "end");
    press(cx, window, view, "f3");
    assert!(viewer_open(cx, view), "F3 did not open the viewer");
    press(cx, window, view, "escape");
    assert!(!viewer_open(cx, view), "Esc did not close the viewer");
    press(cx, window, view, "backspace");
    assert_eq!(here(cx, view), home, "backspace did not open the parent");
    press(cx, window, view, "ctrl-t");
    press(cx, window, view, "ctrl-w");
}

#[gpui_kit::test]
#[ignore = "soak: minutes long; run with --run-ignored all (NAVO_SOAK_ROUNDS, NAVO_SOAK_FOLDERS)"]
#[expect(
    clippy::print_stderr,
    reason = "the soak reports its timings on the test log"
)]
fn the_key_loop_keeps_every_counter_at_its_baseline(cx: &mut TestAppContext) {
    let rounds = tunable("NAVO_SOAK_ROUNDS", 200).max(2 * SAMPLE_EVERY);
    let folders = tunable("NAVO_SOAK_FOLDERS", 50_000);
    let built = Instant::now();
    let fs = big_tree(folders);
    eprintln!("soak: built {folders} folders in {:?}", built.elapsed());
    let cache = navo_cache::ListingCache::default();
    let (window, view) = soak_window(cx, fs, cache.clone());
    press(cx, window, &view, "down");
    let started = Instant::now();
    let mut baseline: Option<Sample> = None;
    for done in 1..=rounds {
        round(cx, window, &view);
        if done % SAMPLE_EVERY != 0 {
            continue;
        }
        let now = sample(cx, &view, &cache);
        eprintln!("soak: round {done} after {:?}: {now:?}", started.elapsed());
        match &baseline {
            None => baseline = Some(now),
            Some(first) => assert_eq!(&now, first, "counters grew by round {done}"),
        }
    }
}
