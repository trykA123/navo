use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gpui_kit::{AnyWindowHandle, Entity, TestAppContext, VisualTestContext};
use navo_core::{AppState, Command, Location};

use super::drive_fs::{DriveFs, local};
use super::{fake, open_options, options_for, settle};
use crate::git::{FolderBadges, GitHooks, NoGit};
use crate::window_view::NavoWindow;

#[derive(Default)]
struct Log {
    observed: Vec<(Location, u64, Vec<String>)>,
    forgotten: Vec<Location>,
    focused: usize,
    published: HashMap<String, Arc<FolderBadges>>,
    notify: Option<Box<dyn Fn() + Send + Sync>>,
}

#[derive(Clone, Default)]
struct Fake(Arc<Mutex<Log>>);

impl Fake {
    fn publish(&self, at: &Location, revision: u64) {
        let mut log = self.0.lock().unwrap();
        let badges = Arc::new(FolderBadges {
            revision,
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
        let log = self.0.lock().unwrap();
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
    assert_eq!(log.observed.len(), 1);
}

#[gpui_kit::test]
fn a_new_revision_reaches_the_props_and_an_older_one_is_dropped(cx: &mut TestAppContext) {
    let git = Fake::default();
    let view = open_local(cx, Arc::new(git.clone()));
    let home = local(r"C:\home");
    git.publish(&home, 5);
    settle(cx, &view);
    assert_eq!(pane_badges(cx, &view).map(|b| b.revision), Some(5));
    git.publish(&home, 0);
    settle(cx, &view);
    assert_eq!(pane_badges(cx, &view).map(|b| b.revision), Some(5));
}

#[gpui_kit::test]
fn badges_turned_off_leave_the_gutter_empty_and_turned_on_fill_it_again(cx: &mut TestAppContext) {
    let git = Fake::default();
    let view = open_local(cx, Arc::new(git.clone()));
    let home = local(r"C:\home");
    git.publish(&home, 5);
    settle(cx, &view);
    assert!(pane_badges(cx, &view).is_some());
    git.unpublish(&home);
    settle(cx, &view);
    assert!(pane_badges(cx, &view).is_none());
    git.publish(&home, 6);
    settle(cx, &view);
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
