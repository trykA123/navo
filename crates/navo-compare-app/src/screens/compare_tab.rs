//! The compare tab: owns one `CompareRun`, drains its updates on the UI thread and shows the
//! Folders view over the rows. The shell mounts it in place of the Start tab (H2).

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedReceiver, unbounded};
use gpui_kit::base::{TestSupportExt as _, TestTextExt as _};
use gpui_kit::{AsyncApp, FocusHandle, Focusable, Global, Role, Subscription, WeakEntity};
use navo_compare_cli::args::SideSpec;
use sidewise_host::compare::pane::PaneCompare;
use sidewise_kit::prelude::*;
use sidewise_kit::tokens;
use sidewise_ui::browser::Waker;
use sidewise_ui::window_view::{CompareScreen, LookDefaults};

use super::compare_host::{look_of, new_compare_screen};
use super::compare_labels::side_root;
pub(super) use super::compare_labels::{path_of, title_of};
use super::compare_run::{CompareRun, RunError};
use super::compare_tab_large::large_timer;
use super::folders_view::FoldersView;
use super::git_sides::{shared_sides, target_for};
use super::host_parts::SharedHashes;
use super::large_view::LargeView;
use crate::layers::toast;
use crate::start_tabs::StartChoice;

/// Pause between two wake updates, about one frame.
const PAUSE: Duration = Duration::from_millis(16);

/// Key context of the tab root; the tab keys bind inside it.
pub(super) const CONTEXT: &str = "CompareTab";

/// Extra context token while the tab's own views (Folders, Large) show; the digit keys bind to it,
/// so digits typed inside the compare screen reach Navo.
pub(crate) const OWN: &str = "CompareTabOwn";

/// The tab root's context string while `OWN` is set; keep it equal to `CONTEXT` plus `OWN`.
const OWN_CONTEXTS: &str = "CompareTab CompareTabOwn";

/// The two sides and the start choice a compare tab opens with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompareRequest {
    pub left: SideSpec,
    pub right: SideSpec,
    pub choice: StartChoice,
}

/// Which screen the tab shows; `File` and `Stream` both use the one compare screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabView {
    Large,
    Folders,
    File,
    Stream,
}

/// The app's one folder-compare engine, shared by every compare tab.
struct CompareHooks(Arc<dyn sidewise_ui::browser::CompareHooks>);

impl Global for CompareHooks {}

/// Sets the app's one `Arc<PaneCompare>` global, over the shared Git sides; `window::run` calls it once, tests call it before opening a shell.
pub fn install_compare_hooks(cx: &mut App) {
    cx.set_global(CompareHooks(Arc::new(PaneCompare::new(
        Arc::new(SharedHashes),
        target_for(shared_sides()),
    ))));
}

/// The installed compare engine; `None` before `install_compare_hooks`.
pub(super) fn compare_hooks(cx: &App) -> Option<Arc<dyn sidewise_ui::browser::CompareHooks>> {
    cx.try_global::<CompareHooks>()
        .map(|hooks| Arc::clone(&hooks.0))
}

/// Test hook: replaces the compare engine with a scripted one.
#[cfg(test)]
pub(crate) fn install_test_hooks(hooks: Arc<dyn sidewise_ui::browser::CompareHooks>, cx: &mut App) {
    cx.set_global(CompareHooks(hooks));
}

/// A started run and the receiver of its wake signals, not yet tied to an entity.
pub struct Started {
    run: CompareRun,
    wakes: UnboundedReceiver<()>,
    wake: Waker,
}

/// One compare tab: the run, the shown view, the Folders view, its title and focus handle.
pub struct CompareTab {
    pub(super) run: CompareRun,
    pub(super) view: TabView,
    pub(super) folders: Entity<FoldersView>,
    pub(super) large: Entity<LargeView>,
    /// True while Large shows because the 1,000 ms timer put it there; the session's end then ends it.
    pub(super) auto_large: bool,
    /// Set when a view change needs the window to move focus; `render` does it.
    pub(super) focus_pending: bool,
    pub(super) screen: Entity<CompareScreen>,
    pub(super) shown: Option<u64>,
    /// The text of a compare that did not start; the tab shows it instead of the listing.
    pub(super) failure: Option<SharedString>,
    /// True once `attach` ran: the shell has subscribed, so `SidesOpened` reaches it.
    pub(super) attached: bool,
    /// True once `SidesOpened` was sent; a failed run never sets it, so F5 can still send it.
    pub(super) opened_sent: bool,
    request: CompareRequest,
    title: SharedString,
    focus: FocusHandle,
    pub(super) subscriptions: Vec<Subscription>,
}

/// The tab's undo rewrote these files; the shell reloads them in the other tabs (NC148).
pub struct Rewrote(pub Vec<sidewise_core::Location>);

impl gpui_kit::EventEmitter<Rewrote> for CompareTab {}

/// The tab cleared every Viewed mark; the shell clears them in the other tabs (VD2).
pub struct ClearedViewed;

impl gpui_kit::EventEmitter<ClearedViewed> for CompareTab {}

/// The run opened both sides, so the compare is worth a Recent row; the shell records it.
pub struct SidesOpened;

impl gpui_kit::EventEmitter<SidesOpened> for CompareTab {}

impl CompareTab {
    /// Starts the run for `request`; a bad side returns `RunError` before any entity exists.
    /// The shell calls this outside `cx.new`, so an error leaves no half-made tab behind.
    pub fn start(request: &CompareRequest, cx: &App) -> Result<Started, RunError> {
        let hooks = compare_hooks(cx)
            .ok_or_else(|| RunError::Location("the compare engine is not installed".to_owned()))?;
        let (sender, wakes) = unbounded();
        let wake: Waker = Arc::new(move || {
            let _ = sender.unbounded_send(());
        });
        let run = CompareRun::start(
            (&request.left, &request.right),
            &request.choice,
            &*hooks,
            Arc::clone(&wake),
        )?;
        Ok(Started { run, wakes, wake })
    }

    /// Makes the tab over a started run and arms the wake loop.
    pub fn from_started(
        started: Started,
        request: &CompareRequest,
        cx: &mut Context<Self>,
    ) -> Self {
        let Started { run, wakes, wake } = started;
        let roots = [&request.left, &request.right].map(side_root);
        let folders = cx.new(|cx| FoldersView::new(cx).with_roots(roots));
        let large = cx.new(LargeView::new);
        let look = look_of(&crate::shell_prefs::prefs(cx));
        let screen = cx.new(|_| new_compare_screen(wake, look));
        cx.spawn(async move |this, cx| pump(wakes, this, cx).await)
            .detach();
        cx.spawn(async move |this, cx| large_timer(this, cx).await)
            .detach();
        let mut tab = Self {
            run,
            view: TabView::Folders,
            folders,
            large,
            auto_large: false,
            focus_pending: false,
            screen,
            shown: None,
            failure: None,
            attached: false,
            opened_sent: false,
            request: request.clone(),
            title: title_of(request),
            focus: cx.focus_handle(),
            subscriptions: Vec::new(),
        };
        tab.on_wake(cx);
        tab
    }

    /// Starts the run and builds the tab; for a caller that already holds the entity's context.
    #[expect(
        dead_code,
        reason = "the shell opens tabs through `start`, `from_started` and `attach`"
    )]
    pub fn new(
        request: CompareRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<Self, RunError> {
        let started = Self::start(&request, cx)?;
        let mut tab = Self::from_started(started, &request, cx);
        tab.attach(&request, window, cx);
        Ok(tab)
    }

    /// What the tab was opened with; the shell compares it with a Recent row's compare.
    pub fn request(&self) -> &CompareRequest {
        &self.request
    }

    /// The tab's title: both folder names around an arrow.
    pub fn title(&self) -> SharedString {
        self.title.clone()
    }

    /// The Folders view entity (lets tests and the shell read its state).
    pub fn folders(&self) -> &Entity<FoldersView> {
        &self.folders
    }

    /// The Large view entity; for tests.
    #[cfg(test)]
    pub(crate) fn large(&self) -> &Entity<LargeView> {
        &self.large
    }

    /// Gives the file diff screen a new saved look and redraws.
    pub fn set_look(&mut self, look: LookDefaults, cx: &mut Context<Self>) {
        self.screen.update(cx, |screen, cx| {
            screen.set_look(look);
            cx.notify();
        });
        cx.notify();
    }

    /// The compare screen entity that shows the file diff.
    pub fn screen(&self) -> &Entity<CompareScreen> {
        &self.screen
    }

    /// The shown view.
    pub fn view(&self) -> TabView {
        self.view
    }

    /// The id of the pair last opened in the file diff.
    pub fn shown_pair(&self) -> Option<u64> {
        self.shown
    }

    /// The id of the session row at `path`; for tests.
    #[cfg(test)]
    pub(crate) fn row_id(&self, path: &str) -> Option<u64> {
        let rows = self.run.session()?.rows_under_folder();
        rows.iter().find(|row| row.path == path).map(|row| row.id)
    }

    /// How many lines the Folders view shows; for tests.
    #[cfg(test)]
    pub(crate) fn folder_line_count(&self, cx: &App) -> usize {
        self.folders.read(cx).line_count()
    }

    /// Focuses the shown view's keys.
    pub fn focus_view(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.failure.is_some() {
            self.focus.focus(window, cx);
            return;
        }
        match self.view {
            TabView::Folders => self.folders.read(cx).focus_handle(cx).focus(window, cx),
            TabView::File => {
                let focused = self
                    .screen
                    .update(cx, |screen, cx| screen.focus_file_screen(window, cx));
                if !focused {
                    self.focus.focus(window, cx);
                }
            }
            TabView::Large => self.large.read(cx).focus_handle(cx).focus(window, cx),
            TabView::Stream => self.focus.focus(window, cx),
        }
    }

    /// Applies queued updates: new session rows go to the Folders view, then the compare screen
    /// polls its diff inboxes. Notifies when either changed.
    fn on_wake(&mut self, cx: &mut Context<Self>) {
        let drained = self.run.drain();
        if drained && let Some(session) = self.run.session() {
            let rows = session.rows_under_folder();
            self.folders.update(cx, |view, cx| view.set_rows(rows, cx));
            self.sync_large(cx);
        }
        if drained {
            self.note_failure(cx);
        }
        self.note_opened(cx);
        let polled = self
            .screen
            .update(cx, |screen, cx| screen.poll_compare_screen(false, cx));
        if drained || polled {
            cx.notify();
        }
    }

    /// Toasts a compare that did not start, once per run, and shows its text in the tab.
    fn note_failure(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.run.failure().map(SharedString::from) else {
            return;
        };
        if self.failure.is_none() {
            toast::notify(text.clone(), cx);
            self.failure = Some(text);
            self.focus_pending = true;
        }
    }
}

/// Applies wakes to the tab until the wake channel or the tab closes. Queued wakes merge into
/// one update, and a 16 ms pause after it lets the UI paint while a large walk keeps sending.
async fn pump(mut wakes: UnboundedReceiver<()>, this: WeakEntity<CompareTab>, cx: &mut AsyncApp) {
    while wakes.next().await.is_some() {
        while wakes.try_recv().is_ok() {}
        // A failed upgrade means the tab closed; the walk's thread may still wake.
        if this.update(cx, |tab, cx| tab.on_wake(cx)).is_err() {
            return;
        }
        cx.background_executor().timer(PAUSE).await;
    }
}

impl Render for CompareTab {
    /// The shown view inside the tab's key context.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.focus_pending) {
            self.focus_view(window, cx);
        }
        let shown = match (&self.failure, self.view) {
            (Some(text), _) => failure_view(text, cx).into_any_element(),
            (None, TabView::Folders) => self.folders.clone().into_any_element(),
            (None, TabView::Large) => self.large.clone().into_any_element(),
            (None, TabView::File | TabView::Stream) => self.screen.clone().into_any_element(),
        };
        let own = self.failure.is_some() || matches!(self.view, TabView::Folders | TabView::Large);
        let root = div()
            .key_context(if own { OWN_CONTEXTS } else { CONTEXT })
            .track_focus(&self.focus)
            // Navo's diff and Stream roots are `flex_1`: they need a flex parent to get a height.
            .flex()
            .flex_col()
            .size_full()
            .child(shown);
        self.with_keys(root, cx)
    }
}

/// The error state in place of the listing: the reason the compare did not start, in the
/// warning colour, announced as an alert.
fn failure_view(text: &SharedString, cx: &App) -> impl IntoElement {
    let t = tokens::current(cx);
    div()
        .id("compare-failed")
        .role(Role::Alert)
        .aria_label(text.clone())
        .test_support()
        .flex_1()
        .flex()
        .items_center()
        .justify_center()
        .p(t.spacing.md)
        .text_color(t.colors.warn)
        .child(text.clone().test_text(text.clone()))
}
