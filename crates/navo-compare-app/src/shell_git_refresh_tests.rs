//! View tests of Git compare tabs on the real compare engine (G16): a side that fails to open
//! shows its problem, and F5 re-resolves a moved branch in one tab only.

use std::fs;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Entity, Modifiers, TestAppContext, VisualTestContext,
};
use navo_compare_cli::args::SideSpec;
use sidewise_core::Location;
use sidewise_git::fixture::Fixture;
use sidewise_git::location::{Form, GitAt};
use sidewise_host::compare::pane::PaneCompare;
use sidewise_ui::browser::{CompareControl, CompareHooks, CompareSink, CompareUpdate};
use sidewise_ui::compare_diff::CompareOptions;

use crate::compare_tab_tests::{TempDir, shell};
use crate::screens::compare_tab::{CompareRequest, install_test_hooks};
use crate::screens::git_sides::{GitSides, target_for};
use crate::screens::host_parts::NoHashes;
use crate::shell::Shell;
use crate::start_tabs::StartChoice;

type Opened = (AnyWindowHandle, Entity<Shell>);

/// One compare the relay started: the tab's sink and the updates the engine sent so far.
struct Run {
    sink: CompareSink,
    held: Arc<Mutex<Vec<CompareUpdate>>>,
}

/// The real compare engine whose updates wait in a buffer until the test thread hands them to
/// the tab, so no engine thread wakes the GPUI test scheduler.
struct Relay {
    engine: PaneCompare,
    runs: Arc<Mutex<Vec<Run>>>,
}

impl CompareHooks for Relay {
    /// Every location can be compared in this fixture.
    fn supports(&self, _location: &Location) -> bool {
        true
    }

    /// Starts the real engine with a buffering sink and keeps the tab's sink for `settle`.
    fn start(
        &self,
        sides: (&Location, &Location),
        options: &CompareOptions,
        sink: CompareSink,
    ) -> Box<dyn CompareControl> {
        let held = Arc::new(Mutex::new(Vec::new()));
        let buffer = Arc::clone(&held);
        let buffering: CompareSink = Arc::new(move |update| lock(&buffer).push(update));
        let control = self.engine.start(sides, options, buffering);
        lock(&self.runs).push(Run { sink, held });
        control
    }
}

/// Locks a test buffer, taking it back from a poisoned lock.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Installs the real engine over one isolated Git side cache; returns its run list.
fn install_relay(cx: &mut TestAppContext) -> Arc<Mutex<Vec<Run>>> {
    install_relay_over(cx, Arc::new(GitSides::isolated()))
}

/// As `install_relay`, over the given cache, so a test can look at the sides it holds.
fn install_relay_over(cx: &mut TestAppContext, sides: Arc<GitSides>) -> Arc<Mutex<Vec<Run>>> {
    let runs = Arc::new(Mutex::new(Vec::new()));
    let engine = PaneCompare::new(Arc::new(NoHashes), target_for(sides));
    let relay = Relay {
        engine,
        runs: Arc::clone(&runs),
    };
    cx.update(|cx| install_test_hooks(Arc::new(relay), cx));
    runs
}

/// Waits (bounded) until the newest run finished or failed, then hands every run's updates to
/// its tab on the test thread and renders.
fn settle(cx: &mut TestAppContext, runs: &Mutex<Vec<Run>>, window: AnyWindowHandle) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let done = lock(runs).last().is_some_and(|run| {
            lock(&run.held).iter().any(|update| {
                matches!(
                    update,
                    CompareUpdate::Finished { .. } | CompareUpdate::Failed(_)
                )
            })
        });
        if done {
            break;
        }
        assert!(Instant::now() < deadline, "the compare did not end");
        std::thread::sleep(Duration::from_millis(10));
    }
    for run in lock(runs).iter() {
        let updates: Vec<CompareUpdate> = lock(&run.held).drain(..).collect();
        for update in updates {
            (run.sink)(update);
        }
    }
    // The tab's wake pump pauses 16 ms after each update; let that pause pass.
    cx.executor().advance_clock(Duration::from_millis(20));
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
}

/// Opens `request` in the tab at `index` and lets its compare end.
fn open_at(
    cx: &mut TestAppContext,
    (window, shell): &Opened,
    runs: &Mutex<Vec<Run>>,
    (index, request): (usize, CompareRequest),
) {
    cx.update_window(*window, |_, window, cx| {
        shell.update(cx, |shell, cx| {
            shell.open_compare((index, request), window, cx)
        })
    })
    .expect("window")
    .expect("the tab opens; the Git side opens on the session thread");
    settle(cx, runs, *window);
}

/// Whether the compare tab at `index` lists a row at `path`.
fn has_row(cx: &mut TestAppContext, shell: &Entity<Shell>, index: usize, path: &str) -> bool {
    shell.read_with(cx, |shell, cx| {
        shell.compare_tabs()[index].read(cx).row_id(path).is_some()
    })
}

/// A request for `repo@rev` against a folder.
fn git_request(repo: &Fixture, rev: &str, folder: &TempDir) -> CompareRequest {
    CompareRequest {
        left: SideSpec::GitRev {
            repo: repo.path().to_path_buf(),
            rev: rev.into(),
        },
        right: SideSpec::Path(folder.path().to_path_buf()),
        choice: StartChoice::default(),
    }
}

#[gpui_kit::test]
fn a_git_side_with_no_such_revision_shows_its_problem_not_an_empty_listing(
    cx: &mut TestAppContext,
) {
    let repo = Fixture::new();
    let _ = repo.commit("main", &[("a.txt", b"a")]);
    let folder = TempDir::new("g16-bad");
    fs::write(folder.path().join("a.txt"), b"a").expect("write");
    let opened = shell(cx);
    let runs = install_relay(cx);
    open_at(cx, &opened, &runs, (0, git_request(&repo, "h1", &folder)));
    let (window, shell) = &opened;
    let toasts = shell.read_with(cx, |shell, cx| shell.toast_host().read(cx).messages());
    assert!(
        toasts
            .iter()
            .any(|text| text.as_ref() == "No such revision"),
        "toasts were {toasts:?}"
    );
    let shown = cx
        .update_window(*window, |_, window, _| {
            window.try_find("compare-failed").is_some()
        })
        .expect("window");
    assert!(shown, "the tab shows the error state, not an empty listing");
}

#[gpui_kit::test]
fn f5_shows_a_new_commit_and_an_older_tab_on_the_same_branch_keeps_its_rows(
    cx: &mut TestAppContext,
) {
    let repo = Fixture::new();
    let _ = repo.commit("main", &[("a.txt", b"a")]);
    let folder = TempDir::new("g16-f5");
    fs::write(folder.path().join("a.txt"), b"a").expect("write");
    let opened = shell(cx);
    let sides = Arc::new(GitSides::isolated());
    let runs = install_relay_over(cx, Arc::clone(&sides));
    open_at(cx, &opened, &runs, (0, git_request(&repo, "main", &folder)));
    let at = GitAt {
        repo: repo.path().to_path_buf(),
        rev: "main".into(),
        inner: String::new(),
        form: Form::Checkout,
    };
    let old = sides.side(&at).expect("the first tab's side");
    let old_commit = old.commit();
    let (window, shell) = &opened;
    cx.update_window(*window, |_, window, cx| {
        shell.update(cx, |shell, cx| shell.new_tab(window, cx));
    })
    .expect("window");
    open_at(cx, &opened, &runs, (1, git_request(&repo, "main", &folder)));
    assert!(has_row(cx, shell, 1, "a.txt"), "the second tab lists a.txt");
    let _ = repo.commit("main", &[("a.txt", b"a"), ("b.txt", b"b")]);
    let before = lock(&runs).len();
    cx.simulate_keystrokes(*window, "f5");
    assert_eq!(lock(&runs).len(), before + 1, "F5 starts the compare again");
    settle(cx, &runs, *window);
    assert!(has_row(cx, shell, 1, "b.txt"), "F5 shows the new commit");
    assert!(
        has_row(cx, shell, 0, "a.txt"),
        "the first tab keeps its rows"
    );
    assert!(
        !has_row(cx, shell, 0, "b.txt"),
        "the first tab keeps its commit"
    );
    assert!(
        has_row(cx, shell, 1, "a.txt"),
        "the second tab still lists a.txt"
    );
    let newest = sides.side(&at).expect("the newest side");
    assert!(!Arc::ptr_eq(&old, &newest), "F5 opened a new side");
    assert_eq!(old.commit(), old_commit, "the older side is unchanged");
    assert_ne!(
        newest.commit(),
        old_commit,
        "the new side shows the new commit"
    );
}

#[gpui_kit::test]
fn f5_empties_the_folders_view_until_the_new_walk_sends_rows(cx: &mut TestAppContext) {
    let repo = Fixture::new();
    let _ = repo.commit("main", &[("a.txt", b"a")]);
    let folder = TempDir::new("g16-f5-empty");
    fs::write(folder.path().join("a.txt"), b"a").expect("write");
    let opened = shell(cx);
    let runs = install_relay(cx);
    open_at(cx, &opened, &runs, (0, git_request(&repo, "main", &folder)));
    let (window, shell) = &opened;
    let lines = |cx: &mut TestAppContext| {
        shell.read_with(cx, |shell, cx| {
            shell.compare_tabs()[0].read(cx).folder_line_count(cx)
        })
    };
    assert!(lines(cx) > 0, "the tab lists its rows first");
    cx.simulate_keystrokes(*window, "f5");
    assert_eq!(lines(cx), 0, "no stale row ids before the first drain");
}

#[gpui_kit::test]
fn f5_compares_again_from_the_filter_field_and_keeps_the_filter_text(cx: &mut TestAppContext) {
    let repo = Fixture::new();
    let _ = repo.commit("main", &[("a.txt", b"a")]);
    let folder = TempDir::new("tn3-f5-filter");
    fs::write(folder.path().join("a.txt"), b"a").expect("write");
    let opened = shell(cx);
    let runs = install_relay(cx);
    open_at(cx, &opened, &runs, (0, git_request(&repo, "main", &folder)));
    let (window, shell) = &opened;
    let center = cx
        .update_window(*window, |_, window, _| {
            window.find("top-search").bounds().center()
        })
        .expect("window");
    VisualTestContext::from_window(*window, cx).simulate_click(center, Modifiers::none());
    cx.simulate_input(*window, "a.t");
    let filter = |cx: &mut TestAppContext| shell.read_with(cx, |shell, cx| shell.topbar_filter(cx));
    assert_eq!(filter(cx), "a.t", "the filter field has focus and its text");
    let before = lock(&runs).len();
    cx.simulate_keystrokes(*window, "f5");
    assert_eq!(lock(&runs).len(), before + 1, "F5 compares again");
    assert_eq!(filter(cx), "a.t", "the filter text stays");
}
