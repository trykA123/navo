use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui_kit::test::TestWindowExt;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use navo_core::{AppState, Location};

use super::drive_fs::{DriveFs, local};
use super::{fake, open_options, options_for, press, settle};
use crate::git::menu::Choice;
use crate::git::{
    BadgeKind, FolderBadges, GitAction, GitHooks, GitMenu, GitMenuItem, GitStarted, Previewed,
    RepoBadge, RepoRoot, ToolRow,
};
use crate::window_view::NavoWindow;

#[derive(Default)]
pub(super) struct Log {
    menus: Vec<Location>,
    runs: Vec<(Location, GitAction)>,
    /// The confirmation text and id each `run_confirmed` named.
    pub(super) shown: Vec<(Option<String>, Option<String>)>,
    rescans: Vec<Location>,
    outcome: Option<Result<GitStarted, String>>,
    pub(super) refreshes: usize,
    pub(super) refusal: Option<String>,
}

type Notify = Arc<Mutex<Option<Arc<dyn Fn() + Send + Sync>>>>;
type Held = Vec<(Location, Box<dyn FnOnce(Previewed) + Send>)>;

#[derive(Clone)]
pub(super) struct Fake {
    pub(super) log: Arc<Mutex<Log>>,
    held: Arc<Mutex<Held>>,
    items: Vec<GitMenuItem>,
    badges: Arc<Mutex<Arc<FolderBadges>>>,
    /// The highest generation `observe` was given: the badges carry it as their revision,
    /// as the real hooks do, so a listing that reloads never outdates them.
    generation: Arc<AtomicU64>,
    notify: Notify,
}

impl Fake {
    pub(super) fn new(items: Vec<GitMenuItem>, badge: RepoBadge) -> Self {
        let by_name = HashMap::from([("docs".to_owned(), badge)]);
        Self {
            log: Arc::default(),
            held: Arc::default(),
            items,
            badges: Arc::new(Mutex::new(Arc::new(FolderBadges {
                revision: 1,
                by_name,
                ..FolderBadges::default()
            }))),
            generation: Arc::new(AtomicU64::new(0)),
            notify: Arc::default(),
        }
    }

    /// Makes the `docs` row a folder inside the repository `name`, whose root badge is `root`.
    pub(super) fn inside(self, name: &str, root: RepoBadge, below: RepoBadge) -> Self {
        let roots = HashMap::from([(
            "docs".to_owned(),
            RepoRoot {
                name: name.to_owned(),
                badge: root,
            },
        )]);
        *self.badges.lock().unwrap() = Arc::new(FolderBadges {
            revision: 1,
            by_name: HashMap::from([("docs".to_owned(), below)]),
            roots,
            ..FolderBadges::default()
        });
        self
    }

    pub(super) fn requests(&self) -> usize {
        self.held.lock().unwrap().len()
    }

    pub(super) fn reply(&self, replies: Previewed) {
        let (_, done) = self.held.lock().unwrap().remove(0);
        done(replies);
    }

    pub(super) fn runs(&self) -> Vec<GitAction> {
        let log = self.log.lock().unwrap();
        log.runs.iter().map(|(_, action)| *action).collect()
    }
}

impl GitHooks for Fake {
    fn observe(&self, _: &Location, generation: u64, _: Vec<String>) {
        if self.generation.fetch_max(generation, Ordering::SeqCst) < generation {
            let notify = self.notify.lock().unwrap().clone();
            if let Some(notify) = notify {
                notify();
            }
        }
    }
    fn forget(&self, _: &Location) {}
    fn badges(&self, _: &Location) -> Option<Arc<FolderBadges>> {
        let revision = self.generation.load(Ordering::SeqCst);
        let mut badges = self.badges.lock().unwrap();
        if badges.revision != revision {
            *badges = Arc::new(FolderBadges {
                revision,
                slow_drive: badges.slow_drive,
                by_name: badges.by_name.clone(),
                roots: badges.roots.clone(),
            });
        }
        Some(Arc::clone(&badges))
    }
    fn subscribe(&self, notify: Box<dyn Fn() + Send + Sync>) {
        let notify: Arc<dyn Fn() + Send + Sync> = Arc::from(notify);
        *self.notify.lock().unwrap() = Some(Arc::clone(&notify));
        notify();
    }
    fn rescan(&self, listing: &Location) {
        self.log.lock().unwrap().rescans.push(listing.clone());
    }
    fn menu(&self, repo: &Location) -> GitMenu {
        self.log.lock().unwrap().menus.push(repo.clone());
        GitMenu {
            head: "docs".to_owned(),
            items: self.items.clone(),
        }
    }
    fn previews(&self, repo: &Location, done: Box<dyn FnOnce(Previewed) + Send>) {
        self.held.lock().unwrap().push((repo.clone(), done));
    }
    fn run(&self, repo: &Location, action: GitAction) -> Result<GitStarted, String> {
        let mut log = self.log.lock().unwrap();
        log.runs.push((repo.clone(), action));
        log.outcome.clone().unwrap_or(Ok(GitStarted {
            tool: "Fake".to_owned(),
            result: None,
        }))
    }
    fn run_confirmed(
        &self,
        repo: &Location,
        action: GitAction,
        shown: Option<&str>,
        confirm_id: Option<&str>,
    ) -> Result<GitStarted, String> {
        let shown = (shown.map(str::to_owned), confirm_id.map(str::to_owned));
        self.log.lock().unwrap().shown.push(shown);
        self.run(repo, action)
    }
    fn tools(&self) -> Vec<ToolRow> {
        vec![ToolRow {
            id: "fake".to_owned(),
            label: "Fake".to_owned(),
            path: r"C:\Tools\fake.exe".to_owned(),
            selected: true,
        }]
    }
    fn check_command(&self, _: &str) -> Result<(), String> {
        self.log.lock().unwrap().refusal.clone().map_or(Ok(()), Err)
    }
    fn refresh_tools(&self) {
        self.log.lock().unwrap().refreshes += 1;
    }
    fn can_diff(&self, _: &Location) -> bool {
        true
    }
    fn open_diff(&self, _: &Location, _: &Location) -> Result<GitStarted, String> {
        Err(String::new())
    }
}

pub(super) fn item(action: GitAction, label: &str, confirm: Option<&str>) -> GitMenuItem {
    GitMenuItem {
        action,
        label: label.to_owned(),
        confirm: confirm.map(str::to_owned),
        confirm_id: None,
        pending: false,
    }
}

pub(super) fn pending_items() -> Vec<GitMenuItem> {
    let mut items = menu_items(false);
    for item in &mut items {
        item.pending = matches!(item.action, GitAction::Pull | GitAction::Push);
    }
    items
}

pub(super) fn menu_items(confirm: bool) -> Vec<GitMenuItem> {
    let ask = |text| confirm.then_some(text);
    vec![
        item(GitAction::Commit, "Commit…", None),
        item(
            GitAction::Pull,
            "Pull…",
            ask("Pull main from origin: 2 commits"),
        ),
        item(
            GitAction::Push,
            "Push…",
            ask("Push main to origin: 1 commit"),
        ),
        item(GitAction::Log, "Log", None),
    ]
}

pub(super) fn badge(kind: BadgeKind) -> RepoBadge {
    RepoBadge {
        kind,
        is_root: true,
        stale: false,
        branch: Some("main".to_owned()),
        upstream: Some("origin/main".to_owned()),
        ahead: 0,
        behind: 0,
        changed: 0,
        untracked: 0,
        conflicted: 0,
        fetched_ms: None,
        reason: None,
    }
}

pub(super) fn modified() -> RepoBadge {
    RepoBadge {
        changed: 2,
        ..badge(BadgeKind::Modified)
    }
}

pub(super) fn behind() -> RepoBadge {
    RepoBadge {
        behind: 2,
        ..badge(BadgeKind::Behind)
    }
}

pub(super) fn open(cx: &mut TestAppContext, git: &Fake) -> (AnyWindowHandle, Entity<NavoWindow>) {
    let state = AppState::new(local(r"C:\home"));
    let mut options = options_for(fake(), state);
    options.provider = Arc::new(DriveFs::new(None));
    options.git = Some(Arc::new(git.clone()));
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    wait_for_badges(cx, &view);
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
    (window, view)
}

/// Badges reach the view through the lane after the listing, where `settle` cannot see them.
fn wait_for_badges(cx: &mut TestAppContext, view: &Entity<NavoWindow>) {
    let deadline = Instant::now() + Duration::from_secs(30);
    let missing = |cx: &mut TestAppContext| {
        view.read_with(cx, |view, _| {
            let pane = view.browser.state().focused_pane();
            view.git_badges.badges(pane).is_none()
        })
    };
    while missing(cx) {
        assert!(
            Instant::now() < deadline,
            "the focused pane never got its Git badges"
        );
        view.update(cx, |view, cx| view.wake(cx));
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub(super) fn click_badge(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
) {
    let pane = view.read_with(cx, |view, _| view.browser.state().focused_pane());
    cx.update_window(window, |_, window, cx| {
        let mut list = window.within(("list", pane.get()));
        list.within(("row", 0usize)).click("git-badge", cx);
    })
    .expect("window");
    settle(cx, view);
    cx.update_window(window, |_, window, cx| window.render_frame(cx))
        .expect("window");
}

pub(super) fn is_open(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> bool {
    view.read_with(cx, |view, cx| view.git_menu.is_open(cx))
}

fn list_focused(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    view: &Entity<NavoWindow>,
) -> bool {
    let handle = view.update(cx, |view, cx| {
        let pane = view.browser.state().focused_pane();
        view.handle(pane, cx)
    });
    cx.update_window(window, |_, window, _| handle.is_focused(window))
        .expect("window")
}

pub(super) fn notice(cx: &mut TestAppContext, view: &Entity<NavoWindow>) -> Option<(String, bool)> {
    view.read_with(cx, |view, _| {
        let shown = view.clipboard_notice()?;
        Some((shown.text.to_string(), shown.is_error))
    })
}

#[gpui_kit::test]
fn a_click_on_the_badge_opens_the_menu_for_that_row(cx: &mut TestAppContext) {
    let git = Fake::new(menu_items(false), modified());
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    assert!(is_open(cx, &view));
    let log = git.log.lock().unwrap();
    assert_eq!(log.menus, [local(r"C:\home\docs")]);
    drop(log);
    let shown = cx
        .update_window(window, |_, window, _| window.try_find("git-menu").is_some())
        .expect("window");
    assert!(shown);
}

#[gpui_kit::test]
fn escape_closes_the_menu_and_gives_the_list_its_focus_back(cx: &mut TestAppContext) {
    let git = Fake::new(menu_items(false), modified());
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    assert!(!list_focused(cx, window, &view));
    press(cx, window, &view, "escape");
    assert!(!is_open(cx, &view));
    assert!(list_focused(cx, window, &view));
    assert!(git.runs().is_empty());
}

#[gpui_kit::test]
fn alt_g_opens_the_menu_on_the_cursor_row(cx: &mut TestAppContext) {
    let git = Fake::new(menu_items(false), modified());
    let (window, view) = open(cx, &git);
    view.update(cx, |view, cx| {
        let pane = view.browser.state().focused_pane();
        let handle = view.handle(pane, cx);
        let _ = window.update(cx, |_, window, cx| window.focus(&handle, cx));
    });
    press(cx, window, &view, "alt-g");
    assert!(is_open(cx, &view));
    assert_eq!(git.log.lock().unwrap().menus, [local(r"C:\home\docs")]);
}

#[gpui_kit::test]
fn enter_runs_the_lead_item_without_asking_when_it_has_no_confirmation(cx: &mut TestAppContext) {
    let git = Fake::new(menu_items(false), modified());
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert_eq!(git.runs(), [GitAction::Commit]);
    assert!(!is_open(cx, &view));
    assert_eq!(
        notice(cx, &view),
        Some(("Commit sent to Fake".to_owned(), false))
    );
}

#[gpui_kit::test]
fn a_confirmed_pull_runs_once_and_a_cancelled_one_never_runs(cx: &mut TestAppContext) {
    let git = Fake::new(menu_items(true), behind());
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    press(cx, window, &view, "enter");
    assert!(git.runs().is_empty());
    press(cx, window, &view, "escape");
    settle(cx, &view);
    assert!(git.runs().is_empty());
    click_badge(cx, window, &view);
    press(cx, window, &view, "enter");
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert_eq!(git.runs(), [GitAction::Pull]);
    let shown = git.log.lock().unwrap().shown.clone();
    let text = Some("Pull main from origin: 2 commits".to_owned());
    assert_eq!(shown, [(text, None)]);
}

#[gpui_kit::test]
fn a_refused_run_shows_its_reason_in_the_status_line(cx: &mut TestAppContext) {
    let git = Fake::new(menu_items(false), behind());
    git.log.lock().unwrap().outcome = Some(Err("Pull needs a merge; use your Git tool".to_owned()));
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    press(cx, window, &view, "enter");
    settle(cx, &view);
    let shown = notice(cx, &view);
    assert_eq!(
        shown,
        Some(("Pull needs a merge; use your Git tool".to_owned(), true))
    );
}

#[gpui_kit::test]
fn a_finished_run_shows_its_result_text_in_the_status_line(cx: &mut TestAppContext) {
    let git = Fake::new(menu_items(false), behind());
    git.log.lock().unwrap().outcome = Some(Ok(GitStarted {
        tool: "Fake".to_owned(),
        result: Some("Pulled main from origin".to_owned()),
    }));
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert_eq!(
        notice(cx, &view),
        Some(("Pulled main from origin".to_owned(), false))
    );
}

#[gpui_kit::test]
fn a_disabled_item_explains_itself_and_runs_nothing(cx: &mut TestAppContext) {
    let git = Fake::new(menu_items(false), badge(BadgeKind::Clean));
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    press(cx, window, &view, "down");
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert!(git.runs().is_empty());
    assert_eq!(
        notice(cx, &view),
        Some(("Nothing to commit".to_owned(), false))
    );
}

#[gpui_kit::test]
fn check_again_rescans_the_listing(cx: &mut TestAppContext) {
    let git = Fake::new(menu_items(false), badge(BadgeKind::Unknown));
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    press(cx, window, &view, "enter");
    settle(cx, &view);
    assert_eq!(git.log.lock().unwrap().rescans, [local(r"C:\home")]);
    assert!(git.runs().is_empty());
}

#[gpui_kit::test]
fn a_folder_inside_a_repository_gets_the_repository_menu(cx: &mut TestAppContext) {
    let below = RepoBadge {
        is_root: false,
        branch: None,
        upstream: None,
        ..badge(BadgeKind::Clean)
    };
    let git = Fake::new(menu_items(false), below.clone()).inside("navo", modified(), below);
    let (window, view) = open(cx, &git);
    click_badge(cx, window, &view);
    let (title, commit) = view.read_with(cx, |view, _| {
        let model = &view.git_menu.open.as_ref().expect("open").model;
        let commit = model
            .rows
            .iter()
            .find(|row| row.choice == Choice::Run(GitAction::Commit));
        (model.head.title.clone(), commit.map(|row| row.enabled))
    });
    assert_eq!(title, "navo");
    assert_eq!(commit, Some(true));
}

fn focus_list(cx: &mut TestAppContext, window: AnyWindowHandle, view: &Entity<NavoWindow>) {
    view.update(cx, |view, cx| {
        let pane = view.browser.state().focused_pane();
        let handle = view.handle(pane, cx);
        let _ = window.update(cx, |_, window, cx| window.focus(&handle, cx));
    });
}

#[gpui_kit::test]
fn alt_g_on_a_file_inside_a_repository_opens_the_repository_menu(cx: &mut TestAppContext) {
    let below = RepoBadge {
        is_root: false,
        ..badge(BadgeKind::Clean)
    };
    let git = Fake::new(menu_items(false), below.clone()).inside("navo", modified(), below);
    let (window, view) = open(cx, &git);
    focus_list(cx, window, &view);
    press(cx, window, &view, "down");
    press(cx, window, &view, "alt-g");
    assert!(is_open(cx, &view));
    let title = view.read_with(cx, |view, _| {
        view.git_menu
            .open
            .as_ref()
            .expect("open")
            .model
            .head
            .title
            .clone()
    });
    assert_eq!(title, "navo");
    assert_eq!(git.log.lock().unwrap().menus, [local(r"C:\home")]);
}

#[gpui_kit::test]
fn alt_g_outside_a_repository_says_so(cx: &mut TestAppContext) {
    let git = Fake::new(menu_items(false), modified());
    let (window, view) = open(cx, &git);
    focus_list(cx, window, &view);
    press(cx, window, &view, "down");
    press(cx, window, &view, "alt-g");
    assert!(!is_open(cx, &view));
    assert_eq!(
        notice(cx, &view),
        Some(("No Git repository here".to_owned(), false))
    );
}

#[gpui_kit::test]
fn alt_g_with_git_off_points_to_settings(cx: &mut TestAppContext) {
    let state = AppState::new(local(r"C:\home"));
    let mut options = options_for(fake(), state);
    options.provider = Arc::new(DriveFs::new(None));
    let (window, view) = open_options(cx, options);
    settle(cx, &view);
    focus_list(cx, window, &view);
    press(cx, window, &view, "alt-g");
    assert!(!is_open(cx, &view));
    assert_eq!(
        notice(cx, &view),
        Some((
            "Git status is off — turn it on in Settings → Git".to_owned(),
            false
        ))
    );
}
