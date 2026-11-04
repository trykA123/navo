use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gpui_kit::AppContext as _;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, Entity, TestAppContext, VisualTestContext};
use navo_core::{AppState, Command, Location};

use super::drive_fs::{DriveFs, local};
use super::{fake, open_options, options_for, settle};
use crate::git::{BadgeKind, FolderBadges, GitHooks, NoGit, RepoBadge};
use crate::status_bar::StatusNotice;
use crate::window_view::NavoWindow;

#[derive(Default)]
struct Log {
    observed: Vec<(Location, u64, Vec<String>)>,
    forgotten: Vec<Location>,
    focused: usize,
    published: HashMap<String, Arc<FolderBadges>>,
    notify: Option<Box<dyn Fn() + Send + Sync>>,
    reads: usize,
}

#[derive(Clone, Default)]
struct Fake(Arc<Mutex<Log>>);

fn clean_badge() -> RepoBadge {
    RepoBadge {
        kind: BadgeKind::Clean,
        is_root: true,
        stale: false,
        branch: None,
        upstream: None,
        ahead: 0,
        behind: 0,
        changed: 0,
        untracked: 0,
        conflicted: 0,
        fetched_ms: None,
        reason: None,
    }
}

impl Fake {
    fn publish(&self, at: &Location, revision: u64) {
        self.publish_with(at, revision, false);
    }

    fn publish_with(&self, at: &Location, revision: u64, slow_drive: bool) {
        let mut log = self.0.lock().unwrap();
        let badges = Arc::new(FolderBadges {
            revision,
            slow_drive,
            by_name: HashMap::from([("docs".to_owned(), clean_badge())]),
            ..FolderBadges::default()
        });
        log.published.insert(at.as_str().to_owned(), badges);
        if let Some(notify) = &log.notify {
            notify();
        }
    }
}

impl Fake {
    fn unpublish(&self, at: &Location) {
        let mut log = self.0.lock().unwrap();
        log.published.remove(at.as_str());
        if let Some(notify) = &log.notify {
            notify();
        }
    }
}

impl GitHooks for Fake {
    fn observe(&self, listing: &Location, revision: u64, names: Vec<String>) {
        let mut log = self.0.lock().unwrap();
        log.observed.push((listing.clone(), revision, names));
    }
    fn forget(&self, listing: &Location) {
        self.0.lock().unwrap().forgotten.push(listing.clone());
    }
    fn badges(&self, listing: &Location) -> Option<Arc<FolderBadges>> {
        let mut log = self.0.lock().unwrap();
        log.reads += 1;
        log.published.get(listing.as_str()).cloned()
    }
    fn subscribe(&self, notify: Box<dyn Fn() + Send + Sync>) {
        self.0.lock().unwrap().notify = Some(notify);
    }
    fn rescan(&self, _: &Location) {}
    fn focus_regained(&self) {
        self.0.lock().unwrap().focused += 1;
    }
    fn menu(&self, _: &Location) -> crate::git::GitMenu {
        crate::git::GitMenu::default()
    }
    fn run(
        &self,
        _: &Location,
        _: crate::git::GitAction,
    ) -> Result<crate::git::GitStarted, String> {
        Err(String::new())
    }
    fn tools(&self) -> Vec<crate::git::ToolRow> {
        Vec::new()
    }
    fn can_diff(&self, _: &Location) -> bool {
        false
    }
    fn open_diff(&self, _: &Location, _: &Location) -> Result<crate::git::GitStarted, String> {
        Err(String::new())
    }
}

fn open_local(cx: &mut TestAppContext, hooks: Arc<dyn GitHooks>) -> Entity<NavoWindow> {
    open_window(cx, hooks).1
}

fn open_window(
    cx: &mut TestAppContext,
    hooks: Arc<dyn GitHooks>,
) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let state = AppState::new(local(r"C:\home"));
    let mut options = options_for(fake(), state);
    options.provider = Arc::new(DriveFs::new(None));
    options.git = Some(hooks);
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    (window, view)
}

/// Waits out the publish interval, so the next result reads at once.
fn settle_git(cx: &mut TestAppContext, view: &Entity<NavoWindow>) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(100));
    settle(cx, view);
}

fn pane_badges(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Option<Arc<FolderBadges>> {
    view.read_with(cx, |view, _| {
        let pane = view.browser.state().focused_pane();
        view.git_badges.badges(pane)
    })
}

fn go(cx: &mut TestAppContext, view: &Entity<NavoWindow>, to: &str) {
    let location = local(to);
    view.update(cx, |view, _| {
        view.browser
            .command(&Command::PaneNavigate { location })
            .unwrap();
    });
    settle(cx, view);
}

#[gpui_kit::test]
fn a_listing_load_observes_the_folder_names_once_per_generation(cx: &mut TestAppContext) {
    let git = Fake::default();
    let view = open_local(cx, Arc::new(git.clone()));
    view.update(cx, |view, cx| view.wake(cx));
    cx.run_until_parked();
    let log = git.0.lock().unwrap();
    assert_eq!(log.observed.len(), 1);
    assert_eq!(log.observed[0].2, ["docs"]);
}

#[gpui_kit::test]
fn leaving_the_folder_forgets_it(cx: &mut TestAppContext) {
    let git = Fake::default();
    let view = open_local(cx, Arc::new(git.clone()));
    go(cx, &view, r"C:\home\docs");
    let log = git.0.lock().unwrap();
    assert_eq!(log.forgotten, [local(r"C:\home")]);
    assert_eq!(log.observed.len(), 2);
    assert_eq!(log.observed[1].0, local(r"C:\home\docs"));
    assert!(log.observed[1].2.is_empty());
}

#[gpui_kit::test]
fn a_new_revision_reaches_the_props_and_an_older_one_is_dropped(cx: &mut TestAppContext) {
    let git = Fake::default();
    let view = open_local(cx, Arc::new(git.clone()));
    let home = local(r"C:\home");
    git.publish(&home, 5);
    settle_git(cx, &view);
    assert_eq!(pane_badges(cx, &view).map(|b| b.revision), Some(5));
    git.publish(&home, 0);
    settle_git(cx, &view);
    assert_eq!(pane_badges(cx, &view).map(|b| b.revision), Some(5));
}

#[gpui_kit::test]
fn badges_turned_off_clear_the_marks_and_turned_on_show_them_again(cx: &mut TestAppContext) {
    let git = Fake::default();
    let view = open_local(cx, Arc::new(git.clone()));
    let home = local(r"C:\home");
    git.publish(&home, 5);
    settle_git(cx, &view);
    assert!(pane_badges(cx, &view).is_some());
    git.unpublish(&home);
    settle_git(cx, &view);
    assert!(pane_badges(cx, &view).is_none());
    git.publish(&home, 6);
    settle_git(cx, &view);
    assert_eq!(pane_badges(cx, &view).map(|b| b.revision), Some(6));
}

#[gpui_kit::test]
fn a_listing_that_is_not_local_never_calls_the_hooks(cx: &mut TestAppContext) {
    let git = Fake::default();
    let fs = fake();
    let state = AppState::new(fs.location("home"));
    let mut options = options_for(fs, state);
    options.git = Some(Arc::new(git.clone()));
    let (_, view) = open_options(cx, options);
    settle(cx, &view);
    let log = git.0.lock().unwrap();
    assert!(log.observed.is_empty() && log.forgotten.is_empty());
}

#[gpui_kit::test]
fn with_no_git_the_props_stay_empty(cx: &mut TestAppContext) {
    let view = open_local(cx, Arc::new(NoGit));
    assert!(pane_badges(cx, &view).is_none());
}

#[gpui_kit::test]
fn window_activation_tells_the_hooks_once(cx: &mut TestAppContext) {
    let git = Fake::default();
    let (window, _) = open_window(cx, Arc::new(git.clone()));
    let mut visual = VisualTestContext::from_window(window, cx);
    visual.deactivate_window();
    visual.run_until_parked();
    let before = git.0.lock().unwrap().focused;
    visual.update(|window, _| window.activate_window());
    visual.run_until_parked();
    assert_eq!(git.0.lock().unwrap().focused, before + 1);
}

const SLOW: &str = "Git status skipped: slow drive";

fn footer(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<NavoWindow>) -> String {
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window
            .find(("status", pane.get()))
            .label()
            .map(str::to_owned)
    })
    .expect("window")
    .unwrap_or_default()
}

#[gpui_kit::test]
fn a_slow_drive_note_shows_in_the_pane_status_and_clears_on_navigation(cx: &mut TestAppContext) {
    let git = Fake::default();
    let (window, view) = open_window(cx, Arc::new(git.clone()));
    assert!(!footer(cx, window, &view).contains(SLOW));
    git.publish_with(&local(r"C:\home"), 5, true);
    settle_git(cx, &view);
    assert!(footer(cx, window, &view).contains(SLOW));
    go(cx, &view, r"C:\home\docs");
    assert!(!footer(cx, window, &view).contains(SLOW));
}

#[gpui_kit::test]
fn the_slow_drive_note_follows_the_hooks_and_a_normal_listing_never_shows_it(
    cx: &mut TestAppContext,
) {
    let git = Fake::default();
    let (window, view) = open_window(cx, Arc::new(git.clone()));
    let home = local(r"C:\home");
    git.publish_with(&home, 5, false);
    settle_git(cx, &view);
    assert!(!footer(cx, window, &view).contains(SLOW));
    git.publish_with(&home, 6, true);
    settle_git(cx, &view);
    assert!(footer(cx, window, &view).contains(SLOW));
    git.unpublish(&home);
    settle_git(cx, &view);
    assert!(!footer(cx, window, &view).contains(SLOW));
}

#[gpui_kit::test]
fn a_status_notice_replaces_the_slow_drive_note(cx: &mut TestAppContext) {
    let git = Fake::default();
    let (window, view) = open_window(cx, Arc::new(git.clone()));
    git.publish_with(&local(r"C:\home"), 5, true);
    settle_git(cx, &view);
    view.update(cx, |view, _| {
        view.show_notice(StatusNotice {
            text: "Copied".into(),
            is_error: false,
        });
    });
    let label = footer(cx, window, &view);
    assert!(label.contains("Copied") && !label.contains(SLOW));
}

#[gpui_kit::test]
fn results_streaming_in_are_read_once_per_interval_and_the_last_one_wins(cx: &mut TestAppContext) {
    let git = Fake::default();
    let view = open_local(cx, Arc::new(git.clone()));
    let home = local(r"C:\home");
    settle_git(cx, &view);
    let before = git.0.lock().unwrap().reads;
    for revision in 1..=8 {
        git.publish(&home, revision);
        cx.run_until_parked();
    }
    assert_eq!(
        git.0.lock().unwrap().reads - before,
        1,
        "the first is at once"
    );
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(150));
    cx.run_until_parked();
    assert_eq!(
        git.0.lock().unwrap().reads - before,
        2,
        "one read covers the rest"
    );
    assert_eq!(pane_badges(cx, &view).map(|b| b.revision), Some(8));
}

fn badges_are_calm(cx: &mut TestAppContext, view: &Entity<NavoWindow>, git: &Fake, revision: u64) {
    let log = git.0.lock().unwrap();
    assert_eq!(log.observed.len(), 1, "one observe only");
    assert!(log.forgotten.is_empty(), "nothing forgotten");
    drop(log);
    let badges = pane_badges(cx, view).expect("badges stay");
    assert_eq!(badges.revision, revision);
    assert!(!badges.by_name.is_empty() && badges.by_name.values().all(|b| !b.stale));
}

#[gpui_kit::test]
fn a_resort_keeps_the_git_badges_and_does_not_observe_again(cx: &mut TestAppContext) {
    let git = Fake::default();
    let view = open_local(cx, Arc::new(git.clone()));
    git.publish(&local(r"C:\home"), 5);
    settle_git(cx, &view);
    view.update(cx, |view, _| {
        view.browser.sort_by(crate::file_list::SortBy::Size);
    });
    view.update(cx, |view, cx| view.wake(cx));
    settle_git(cx, &view);
    badges_are_calm(cx, &view, &git, 5);
}

#[gpui_kit::test]
fn a_size_arrival_keeps_the_git_badges_and_does_not_observe_again(cx: &mut TestAppContext) {
    let git = Fake::default();
    let view = open_local(cx, Arc::new(git.clone()));
    git.publish(&local(r"C:\home"), 5);
    settle_git(cx, &view);
    let before = view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab().id();
        view.browser.details(tab)
    });
    view.update(cx, |view, _| {
        let tab = view.browser.state().active_tab().id();
        assert!(view.browser.show_folder_size(&crate::browser::Measured {
            tab,
            folder: local(r"C:\home"),
            name: "docs".into(),
            size: Some(navo_fs::FolderSize::apparent_only(9)),
        }));
    });
    let after = view.read_with(cx, |view, _| {
        let tab = view.browser.state().active_tab().id();
        view.browser.details(tab)
    });
    assert!(!Arc::ptr_eq(&before, &after), "the rows were replaced");
    view.update(cx, |view, cx| view.wake(cx));
    settle_git(cx, &view);
    badges_are_calm(cx, &view, &git, 5);
}

#[gpui_kit::test]
fn frames_with_the_same_rows_build_the_folder_names_once(cx: &mut TestAppContext) {
    let git = Fake::default();
    let view = open_local(cx, Arc::new(git.clone()));
    view.update(cx, |view, cx| view.wake(cx));
    let builds = crate::window_view::git_badges::NAME_BUILDS.with(std::cell::Cell::get);
    for _ in 0..5 {
        view.update(cx, |view, cx| view.wake(cx));
    }
    let after = crate::window_view::git_badges::NAME_BUILDS.with(std::cell::Cell::get);
    assert_eq!(after, builds);
}
