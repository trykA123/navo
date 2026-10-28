use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use rusty_core::find::LineHit;
use rusty_core::{AppState, Command, ProviderPath};
use rusty_fs::{Attributes, CancelToken, EntryKind, FakeFs, FakeOp, FsEntry};

use super::*;

const WAIT: Duration = Duration::from_secs(5);

enum Action {
    Batch(FoundBatch),
    Finish,
}

struct Walk {
    cancel: CancelToken,
    actions: Sender<Action>,
    ended: Receiver<bool>,
}

impl Walk {
    fn batch(&self, names: &[&str]) {
        self.actions.send(Action::Batch(batch(names))).unwrap();
    }

    fn finish(self) {
        self.actions.send(Action::Finish).unwrap();
        assert!(!self.ended.recv_timeout(WAIT).unwrap());
    }

    fn cancelled(self) {
        assert!(self.cancel.wait_cancelled(WAIT));
        self.actions.send(Action::Finish).unwrap();
        assert!(self.ended.recv_timeout(WAIT).unwrap());
    }
}

fn batch(names: &[&str]) -> FoundBatch {
    FoundBatch {
        files: names
            .iter()
            .map(|name| rusty_fs::find::FoundFile {
                relative: ProviderPath::from_text(name),
                entry: FsEntry {
                    name: ProviderPath::from_text("a.c"),
                    kind: EntryKind::File,
                    size: 7,
                    modified_ms: None,
                    attributes: Attributes::default(),
                },
                hits: FileHits {
                    hits: vec![LineHit {
                        line: 3,
                        column: 1,
                        text: "needle".into(),
                    }],
                    total: 1,
                    skipped: None,
                },
            })
            .collect(),
        progress: FindProgress {
            matched: names.len() as u64,
            ..FindProgress::default()
        },
    }
}

fn setup() -> (Browser, Arc<FakeFs>, Receiver<Walk>, Receiver<()>) {
    let fs = Arc::new(FakeFs::new().with_watch());
    fs.add_folder("home");
    let (started, walks) = mpsc::channel();
    let (wake, woken) = mpsc::channel();
    let mut browser = Browser::new(
        AppState::new(fs.location("home")),
        fs.clone(),
        Arc::new(move || {
            let _ = wake.send(());
        }),
    );
    browser.find_walk = Arc::new(move |_, _, cancel, sink| {
        let (actions, receive) = mpsc::channel();
        let (ended, observed) = mpsc::channel();
        started
            .send(Walk {
                cancel: cancel.clone(),
                actions,
                ended: observed,
            })
            .unwrap();
        while let Action::Batch(batch) = receive.recv_timeout(WAIT).unwrap() {
            if sink(batch).is_break() {
                break;
            }
        }
        ended.send(cancel.is_cancelled()).unwrap();
        Ok(FindProgress::default())
    });
    (browser, fs, walks, woken)
}

fn start(browser: &mut Browser) -> TabId {
    browser.start_find(
        browser.state.focused_pane(),
        FindQuery::parse("needle"),
        Location::new(r"C:\work").unwrap(),
    )
}

fn poll(browser: &mut Browser, woken: &Receiver<()>) {
    woken.recv_timeout(WAIT).unwrap();
    assert!(browser.poll());
}

fn names(browser: &Browser, tab: TabId) -> Vec<String> {
    browser
        .state
        .tab(tab)
        .unwrap()
        .listing()
        .unwrap()
        .entries()
        .iter()
        .map(|entry| entry.name.clone())
        .collect()
}

#[test]
fn batches_append_in_order_and_keep_hit_lines_and_relative_locations() {
    let (mut browser, _, walks, woken) = setup();
    let tab = start(&mut browser);
    let walk = walks.recv_timeout(WAIT).unwrap();
    walk.batch(&[r"src\z.c"]);
    poll(&mut browser, &woken);
    walk.batch(&[r"src\a.c", r"tests\b.c"]);
    poll(&mut browser, &woken);
    assert_eq!(names(&browser, tab), [r"src\z.c", r"src\a.c", r"tests\b.c"]);
    assert_eq!(browser.details(tab)[1].name.to_text(), r"src\a.c");
    assert_eq!(
        browser.find_results(tab).unwrap().hits[r"src\a.c"].hits[0].line,
        3
    );
    assert_eq!(
        browser
            .state
            .tab(tab)
            .unwrap()
            .entry_location(r"src\a.c")
            .unwrap()
            .as_str(),
        r"C:\work\src\a.c"
    );
    walk.finish();
    poll(&mut browser, &woken);
    assert!(!browser.find_results(tab).unwrap().running());
}

#[test]
fn stop_cancels_and_preserves_results_and_rejects_late_batches() {
    let (mut browser, _, walks, woken) = setup();
    let tab = start(&mut browser);
    let walk = walks.recv_timeout(WAIT).unwrap();
    walk.batch(&[r"src\a.c"]);
    poll(&mut browser, &woken);
    walk.batch(&[r"src\late.c"]);
    woken.recv_timeout(WAIT).unwrap();
    browser.stop_find(tab);
    walk.cancelled();
    browser.poll();
    assert_eq!(names(&browser, tab), [r"src\a.c"]);
    assert!(!browser.find_results(tab).unwrap().running());
    assert!(!browser.loading());
}

#[test]
fn closing_a_results_tab_cancels_its_walk() {
    let (mut browser, _, walks, _) = setup();
    let tab = start(&mut browser);
    let walk = walks.recv_timeout(WAIT).unwrap();
    browser
        .command(&Command::PaneCloseTab { tab: None })
        .unwrap();
    walk.cancelled();
    browser.poll();
    assert!(browser.find_results(tab).is_none());
    assert!(!browser.loading());
}

#[test]
fn rerun_clears_results_and_rejects_the_previous_walks_batches() {
    let (mut browser, _, walks, woken) = setup();
    let tab = start(&mut browser);
    let first = walks.recv_timeout(WAIT).unwrap();
    first.batch(&["old.c"]);
    poll(&mut browser, &woken);
    first.batch(&["late.c"]);
    woken.recv_timeout(WAIT).unwrap();
    browser.rerun_find(tab);
    assert!(names(&browser, tab).is_empty());
    assert!(browser.find_results(tab).unwrap().hits.is_empty());
    first.cancelled();
    let second = walks.recv_timeout(WAIT).unwrap();
    browser.poll();
    while woken.try_recv().is_ok() {}
    second.batch(&["new.c"]);
    poll(&mut browser, &woken);
    assert_eq!(names(&browser, tab), ["new.c"]);
    second.finish();
}

#[test]
fn results_never_request_provider_listing_refresh_or_watching() {
    let (mut browser, fs, walks, _) = setup();
    let tab = start(&mut browser);
    let walk = walks.recv_timeout(WAIT).unwrap();
    let location = browser.state.tab(tab).unwrap().location().clone();
    assert_eq!(location.as_str(), format!("find://{}", tab.get()));
    browser.start();
    browser.command(&Command::PaneRefresh).unwrap();
    browser.refresh_unwatched(std::slice::from_ref(&location));
    let effects = rusty_core::relist_tab(&mut browser.state, tab);
    browser.run(effects);
    browser.poll();
    assert_eq!(fs.calls(location.as_str(), FakeOp::List), 0);
    assert_eq!(fs.calls(location.as_str(), FakeOp::Metadata), 0);
    assert_eq!(fs.calls(location.as_str(), FakeOp::Watch), 0);
    assert!(browser.state.tab(tab).unwrap().pending_request().is_none());
    assert!(browser.error(tab).is_none());
    browser.stop_find(tab);
    walk.cancelled();
}

#[test]
fn scoped_walks_resolve_hits_against_the_supplied_base() {
    let tree = tempfile::tempdir().unwrap();
    let base_path = tree.path().join("base");
    let nested = base_path.join("src");
    let outside = tree.path().join("outside");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(nested.join("a.c"), "needle").unwrap();
    std::fs::write(outside.join("b.c"), "needle").unwrap();
    let (mut browser, _, _, woken) = setup();
    browser.find_walk = walker();
    let base = Location::new(base_path.to_str().unwrap()).unwrap();
    for (scope, expected) in [
        ("src".to_owned(), nested.join("a.c")),
        (outside.to_str().unwrap().to_owned(), outside.join("b.c")),
    ] {
        let query = FindQuery::parse(&format!("needle in:\"{scope}\""));
        let tab = browser.start_find(browser.state.focused_pane(), query, base.clone());
        while browser.find_results(tab).unwrap().running() {
            woken.recv_timeout(WAIT).unwrap();
            browser.poll();
        }
        let result = browser.find_results(tab).unwrap();
        assert!(result.error.is_none(), "{:?}", result.error);
        assert_eq!(result.progress.matched, 1);
        let shown = browser.state.tab(tab).unwrap();
        assert_eq!(shown.base_or_location(), &base);
        let name = &shown.listing().unwrap().get(0).unwrap().name;
        assert_eq!(
            shown.entry_location(name).unwrap().as_str(),
            expected.to_str().unwrap()
        );
    }
}
