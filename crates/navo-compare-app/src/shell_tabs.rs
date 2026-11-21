//! Tab lifecycle of the shell: the tab types, and opening, closing, switching and tracking tabs.
//! A child module of `shell`, so it reads the shell's private fields.

use sidewise_kit::prelude::*;

use super::Shell;
use crate::chrome::topbar_status::{Crumbs, TopbarMode};
use crate::screens::compare_run::RunError;
use crate::screens::compare_tab::{
    ClearedViewed, CompareRequest, CompareTab, Rewrote, SidesOpened, TabView,
};
use crate::start::{StartEvent, StartScreen};

/// The start screen's crumb detail: the two kinds of compare its tabs offer.
pub(super) const START_HINT: &str = "Folder · File";

/// What a tab shows: the start screen or a compare; each tab owns its screen.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum TabKind {
    Start(Entity<StartScreen>),
    Compare(Entity<CompareTab>),
}

/// One tab: the title in the strip and the kind of body it shows.
pub struct ShellTab {
    pub title: SharedString,
    pub kind: TabKind,
}

/// Extracts open tab titles in display order for initializing and syncing [`Sidebar`].
pub(super) fn tab_titles(tabs: &[ShellTab]) -> Vec<SharedString> {
    tabs.iter().map(|tab| tab.title.clone()).collect()
}

#[path = "shell_sessions.rs"]
mod shell_sessions;
use shell_sessions::stored_rows;

/// A fresh start tab titled "New compare", with its own start screen over the stored rows.
pub(super) fn start_tab(window: &mut Window, cx: &mut Context<Shell>) -> ShellTab {
    let cwd = std::env::current_dir().unwrap_or_default();
    let screen = cx.new(|cx| StartScreen::new(stored_rows(cx), cwd, window, cx));
    cx.subscribe_in(&screen, window, |this, screen, event, window, cx| {
        this.on_start((screen, event), window, cx);
    })
    .detach();
    ShellTab {
        title: "New compare".into(),
        kind: TabKind::Start(screen),
    }
}
impl Shell {
    /// Number of open tabs.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "read by the view tests; the start screen reads it later"
        )
    )]
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Index of the active tab.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "read by the view tests; the start screen reads it later"
        )
    )]
    pub fn active(&self) -> usize {
        self.active
    }

    /// The start screen entity ids of all tabs, in order (lets tests tell tabs apart).
    #[cfg(test)]
    pub(crate) fn tab_ids(&self) -> Vec<gpui_kit::EntityId> {
        self.tabs
            .iter()
            .map(|tab| match &tab.kind {
                TabKind::Start(screen) => screen.entity_id(),
                TabKind::Compare(tab) => tab.entity_id(),
            })
            .collect()
    }

    /// The start screen of the active tab.
    pub fn active_screen(&self) -> Option<&Entity<StartScreen>> {
        match self.tabs.get(self.active).map(|tab| &tab.kind) {
            Some(TabKind::Start(screen)) => Some(screen),
            Some(TabKind::Compare(_)) | None => None,
        }
    }

    /// The title of the tab at `index` (lets tests read it).
    #[cfg(test)]
    pub(crate) fn tab_title(&self, index: usize) -> SharedString {
        self.tabs[index].title.clone()
    }

    /// The compare tab when the active tab is one.
    pub(crate) fn active_compare(&self) -> Option<&Entity<CompareTab>> {
        match self.tabs.get(self.active).map(|tab| &tab.kind) {
            Some(TabKind::Compare(tab)) => Some(tab),
            _ => None,
        }
    }

    /// Every open compare tab, in tab order.
    pub(crate) fn compare_tabs(&self) -> Vec<Entity<CompareTab>> {
        self.tabs
            .iter()
            .filter_map(|tab| match &tab.kind {
                TabKind::Compare(tab) => Some(tab.clone()),
                TabKind::Start(_) => None,
            })
            .collect()
    }

    /// The active tab's body: the start screen or the compare tab; empty when no tab is open.
    pub(super) fn stage(&self) -> AnyElement {
        match (self.active_screen(), self.active_compare()) {
            (Some(screen), _) => screen.clone().into_any_element(),
            (None, Some(tab)) => tab.clone().into_any_element(),
            (None, None) => div().into_any_element(),
        }
    }

    /// Routes one start tab's event; the `subscribe_in` made in `start_tab` calls it.
    /// `Compare` opens and records a compare tab in place of that start tab; `Open` reopens a stored session.
    fn on_start(
        &mut self,
        (screen, event): (&Entity<StartScreen>, &StartEvent),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .tabs
            .iter()
            .position(|tab| tab.kind == TabKind::Start(screen.clone()))
        else {
            return;
        };
        let request = match event {
            StartEvent::Open { id } => return self.reopen((index, id), window, cx),
            StartEvent::Forget { id } => return self.forget_row(id, cx),
            StartEvent::Compare {
                left,
                right,
                choice,
            } => CompareRequest {
                left: left.clone(),
                right: right.clone(),
                choice: *choice,
            },
        };
        let _ = self.open_compare((index, request), window, cx);
    }

    /// Replaces the tab at `index` with a compare tab, makes it active and syncs the chrome.
    /// `choice.mode` is not read (`classify` decides from disk). Err: the tabs stay as they were
    /// and one error toast shows the `RunError` text.
    pub(crate) fn open_compare(
        &mut self,
        (index, request): (usize, CompareRequest),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), RunError> {
        if index >= self.tabs.len() {
            let error = RunError::Location(format!("no tab at index {index}"));
            self.toast(error.to_string(), cx);
            return Err(error);
        }
        let started = match CompareTab::start(&request, cx) {
            Ok(started) => started,
            Err(error) => {
                self.toast(error.to_string(), cx);
                return Err(error);
            }
        };
        let tab = cx.new(|cx| CompareTab::from_started(started, &request, cx));
        let recorded = request.clone();
        cx.subscribe(&tab, move |shell, _, _: &SidesOpened, cx| {
            shell.remember(&recorded, cx);
        })
        .detach();
        tab.update(cx, |tab, cx| tab.attach(&request, window, cx));
        cx.observe(&tab, |shell, _, cx| shell.sync_mode(cx))
            .detach();
        cx.subscribe(&tab, |shell, source, Rewrote(targets), cx| {
            shell.reload_others(&source, targets, cx);
        })
        .detach();
        cx.subscribe(&tab, |shell, source, _: &ClearedViewed, cx| {
            shell.clear_viewed_in_others(&source, cx);
        })
        .detach();
        self.tabs[index] = ShellTab {
            title: tab.read(cx).title(),
            kind: TabKind::Compare(tab),
        };
        self.active = index;
        self.sync_chrome(cx);
        self.activated(window, cx);
        Ok(())
    }

    /// Reloads what shows `targets` in every compare tab but `source`, whose undo rewrote them
    /// and which reloaded itself (NC148: the undo stack is process-wide).
    fn reload_others(
        &self,
        source: &Entity<CompareTab>,
        targets: &[sidewise_core::Location],
        cx: &mut Context<Self>,
    ) {
        for tab in self.compare_tabs().iter().filter(|tab| *tab != source) {
            let screen = tab.read(cx).screen().clone();
            screen.update(cx, |screen, cx| screen.reload_written(targets, cx));
        }
    }

    /// Drops the Viewed marks shown by every compare tab but `source`, which cleared them.
    fn clear_viewed_in_others(&self, source: &Entity<CompareTab>, cx: &mut Context<Self>) {
        for tab in self.compare_tabs().iter().filter(|tab| *tab != source) {
            let screen = tab.read(cx).screen().clone();
            screen.update(cx, |screen, cx| screen.viewed_cleared(cx));
        }
    }

    /// Pushes one toast that expires on its own.
    pub(crate) fn toast(&self, text: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.toasts.update(cx, |host, cx| host.push(text, cx));
    }

    /// The toast host, so a test can press Undo outside a shell update.
    #[cfg(test)]
    pub(crate) fn toast_host(&self) -> Entity<crate::layers::toast::ToastHost> {
        self.toasts.clone()
    }

    /// How many toasts are on screen.
    #[cfg(test)]
    pub(crate) fn toast_count(&self, cx: &App) -> usize {
        self.toasts.read(cx).len()
    }

    /// Appends a start tab, makes it active and focuses its left field.
    pub fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab = start_tab(window, cx);
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        self.sync_chrome(cx);
        self.activated(window, cx);
    }

    /// Closes the tab at `index`; the tab to its right (else the new last) becomes active.
    /// A tab left of the active one keeps the active tab. Closing the last tab never closes the
    /// window: a compare tab becomes a fresh New compare tab, a lone start tab stays as it is.
    /// Only closing the active tab moves the focus; closing another tab leaves it. An index out
    /// of range does nothing.
    pub fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            if matches!(self.tabs[0].kind, TabKind::Compare(_)) {
                self.tabs[0] = start_tab(window, cx);
                self.sync_chrome(cx);
                self.activated(window, cx);
            }
            return;
        }
        let was_active = index == self.active;
        self.tabs.remove(index);
        if index < self.active {
            self.active -= 1;
        }
        self.active = self.active.min(self.tabs.len() - 1);
        self.sync_chrome(cx);
        if was_active {
            self.activated(window, cx);
        } else {
            cx.notify();
        }
    }

    /// Makes the tab at `index` active; an index out of range does nothing.
    pub fn select_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index < self.tabs.len() {
            self.active = index;
            self.sync_chrome(cx);
            self.activated(window, cx);
        }
    }

    /// Moves to the next tab, wrapping from the last to the first.
    pub fn next_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tab((self.active + 1) % self.tabs.len(), window, cx);
    }

    /// Moves to the previous tab, wrapping from the first to the last.
    pub fn prev_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.tabs.len();
        self.select_tab((self.active + count - 1) % count, window, cx);
    }

    /// Focuses the active start tab's left field (NS16) or compare tab's view, else the shell; redraws.
    pub(super) fn activated(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.tabs.get(self.active).map(|tab| &tab.kind) {
            Some(TabKind::Start(screen)) => {
                screen.update(cx, |screen, cx| screen.focus_left(window, cx));
            }
            Some(TabKind::Compare(tab)) => tab.update(cx, |tab, cx| tab.focus_view(window, cx)),
            None => self.handle.focus(window, cx),
        }
        cx.notify();
    }

    /// Pushes open tab titles and active index into the sidebar without touching focus, and
    /// closes the View menu, which belongs to the tab it opened on (VM6).
    pub(super) fn sync_chrome(&mut self, cx: &mut Context<Self>) {
        if self.layers.view.is_some() {
            self.set_view_menu(None, cx);
        }
        let titles = tab_titles(&self.tabs);
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.set_tabs(titles, self.active, cx));
        self.sync_mode(cx);
    }

    /// The topbar's mode; for tests.
    #[cfg(test)]
    pub(crate) fn topbar_mode(&self, cx: &App) -> TopbarMode {
        self.topbar.read(cx).mode()
    }

    /// Sets the topbar mode from the active tab (Start, else the compare tab's view) and its
    /// crumbs: the tab's title, plus the "Folder · File" mode hint on a start tab only.
    pub(super) fn sync_mode(&self, cx: &mut Context<Self>) {
        let active = self.tabs.get(self.active);
        let crumbs = Crumbs {
            title: active.map(|tab| tab.title.clone()).unwrap_or_default(),
            detail: match active.map(|tab| &tab.kind) {
                Some(TabKind::Compare(_)) => SharedString::default(),
                _ => START_HINT.into(),
            },
        };
        let mode = match active.map(|tab| &tab.kind) {
            Some(TabKind::Compare(tab)) => match tab.read(cx).view() {
                TabView::Folders => TopbarMode::Folders,
                TabView::File => TopbarMode::File,
                TabView::Stream => TopbarMode::Stream,
                TabView::Large => TopbarMode::Large,
            },
            _ => TopbarMode::Start,
        };
        self.topbar.update(cx, |bar, cx| {
            bar.set_mode(mode, cx);
            bar.set_crumbs(crumbs, cx);
        });
    }
}

#[cfg(test)]
#[path = "shell_session_tests.rs"]
mod session_tests;

#[cfg(test)]
#[path = "shell_recent_tests.rs"]
mod recent_tests;
