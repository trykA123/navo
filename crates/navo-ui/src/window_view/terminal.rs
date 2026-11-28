//! External terminal launcher and built-in terminal drawer host owned by [`NavoWindow`].

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use navo_core::{Location, PaneId};
use navo_term::grid::DEFAULT_SCROLLBACK;
#[cfg(not(test))]
use navo_term::shells::{Probe, detect};
use navo_term::{ShellSpec, Size};

use super::NavoWindow;
use crate::prelude::*;
use crate::primitives::{Splitter, SplitterState};
use crate::settings::{Change, Field, Value, Values};
use crate::terminal::{
    DEFAULT_DRAWER_HEIGHT_PCT, MIN_DRAWER_HEIGHT, SessionSpawner, SpawnOptions, TerminalDrawer,
    TerminalDrawerConfig, TerminalOffNotice, TerminalSession, spawn_default_session,
    title_bar_terminal_button,
};
use crate::tokens;

type Open = Arc<dyn Fn(&Location) -> Result<(), String> + Send + Sync>;

#[cfg(test)]
type TestSpawner = Rc<dyn Fn(Size, &mut Context<TerminalSession>) -> TerminalSession>;

/// Initial cell grid before the first layout measurement refines it.
const INITIAL_SIZE: Size = Size { cols: 80, rows: 24 };

/// Host hooks for launching the configured external terminal in a folder.
pub struct TerminalHooks {
    pub open: Open,
}

impl TerminalHooks {
    /// Returns a no-op hook reporting that external terminals are not configured.
    #[must_use]
    pub fn none() -> Self {
        Self {
            open: Arc::new(|_| Err(navo_l10n::tr("terminal-not-configured"))),
        }
    }
}

/// Per-window built-in terminal state: owns the multi-session drawer view, splitter state,
/// visibility flag, and module-off notice state (D161.1, D161.3).
pub(super) struct TerminalHost {
    drawer: Option<Entity<TerminalDrawer>>,
    splitter: Entity<SplitterState>,
    is_open: bool,
    off_notice: bool,
    #[cfg(test)]
    spawner: Option<TestSpawner>,
    #[cfg(test)]
    session_spawner: Option<SessionSpawner>,
    #[cfg(test)]
    detected_shells: Option<Vec<ShellSpec>>,
}

impl TerminalHost {
    /// Creates an empty terminal host without spawning any session or thread (D161.1).
    pub(super) fn new(values: &Values, cx: &mut App) -> Self {
        let ratio = drawer_pct_to_ratio(values.terminal_drawer_height());
        let default_ratio = drawer_pct_to_ratio(DEFAULT_DRAWER_HEIGHT_PCT);
        let splitter = cx.new(|_| SplitterState::with_default(ratio, default_ratio));
        Self {
            drawer: None,
            splitter,
            is_open: false,
            off_notice: false,
            #[cfg(test)]
            spawner: None,
            #[cfg(test)]
            session_spawner: None,
            #[cfg(test)]
            detected_shells: None,
        }
    }

    fn shutdown(&mut self, cx: &mut App) {
        if let Some(drawer) = self.drawer.take() {
            drawer.update(cx, |drawer, cx| drawer.shutdown_all(cx));
        }
        self.is_open = false;
    }

    fn sync_height(&self, pct: u16, cx: &mut App) {
        let target = drawer_pct_to_ratio(pct);
        self.splitter.update(cx, |state, cx| {
            if (state.ratio() - target).abs() > 0.005 {
                state.set_ratio(target);
                cx.notify();
            }
        });
    }
}

fn drawer_pct_to_ratio(pct: u16) -> f32 {
    1.0 - (f32::from(pct.clamp(10, 90)) / 100.0)
}

fn ratio_to_drawer_pct(ratio: f32) -> u16 {
    let pct = ((1.0 - ratio) * 100.0).round() as i64;
    u16::try_from(pct.clamp(10, 90)).unwrap_or(DEFAULT_DRAWER_HEIGHT_PCT)
}

impl NavoWindow {
    /// Launches the user's configured external terminal in the active pane's local folder.
    pub(super) fn open_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.browser.state();
        let pane = state.focused_pane();
        let folder = state.active_tab().location().clone();
        if !folder.is_local() {
            let error = navo_l10n::tr("terminal-local-only");
            return self.report_pane_error(pane, error.into());
        }
        let open = self.terminal.open.clone();
        self.run_off_thread(
            Box::new(move || {
                let opened = open(&folder);
                Box::new(move |view, _, _| view.terminal_started(pane, opened))
            }),
            window,
            cx,
        );
    }

    fn terminal_started(&mut self, pane: PaneId, opened: Result<(), String>) {
        if let Err(error) = opened {
            self.report_pane_error(pane, error.into());
        }
    }

    /// Handles `Ctrl+\``: shows the module-off notice when disabled, or cycles
    /// hidden -> show+focus -> (unfocused) re-focus -> (focused) hide+pane-focus when enabled.
    pub(super) fn toggle_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.values.terminal_module_on() {
            self.terminal_host.off_notice = true;
            cx.notify();
            return;
        }
        self.terminal_host.off_notice = false;
        if !self.terminal_host.is_open {
            self.show_and_focus_terminal(window, cx);
            return;
        }
        if self.is_terminal_focused_inner(window, cx) {
            self.hide_terminal(window, cx);
        } else {
            self.focus_terminal_drawer(window, cx);
        }
    }

    /// Handles clicking the title-bar Terminal button: opens+focuses when hidden, hides when open.
    fn toggle_terminal_button(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.values.terminal_module_on() {
            self.terminal_host.off_notice = true;
            cx.notify();
            return;
        }
        if self.terminal_host.is_open {
            self.hide_terminal(window, cx);
        } else {
            self.show_and_focus_terminal(window, cx);
        }
    }

    fn show_and_focus_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ensure_terminal_drawer(cx);
        self.terminal_host.is_open = true;
        self.focus_terminal_drawer(window, cx);
        self.cached.refresh_title(cx);
        cx.notify();
    }

    fn hide_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.terminal_host.is_open = false;
        self.focus_core_pane(window, cx);
        self.cached.refresh_title(cx);
        cx.notify();
    }

    fn close_empty_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.terminal_host.drawer = None;
        self.terminal_host.is_open = false;
        self.focus_core_pane(window, cx);
        self.cached.refresh_title(cx);
        cx.notify();
    }

    fn focus_terminal_drawer(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(drawer) = &self.terminal_host.drawer {
            let handle = drawer.read(cx).focus_handle();
            window.focus(&handle, cx);
            cx.notify();
        }
    }

    fn is_terminal_focused_inner(&self, window: &Window, cx: &App) -> bool {
        self.terminal_host
            .drawer
            .as_ref()
            .is_some_and(|drawer| drawer.read(cx).is_focused(window))
    }

    fn ensure_terminal_drawer(&mut self, cx: &mut Context<Self>) {
        if self.terminal_host.drawer.is_some() {
            return;
        }
        let detected_shells = self.detected_terminal_shells();
        let configured_shell: SharedString = self.values.terminal_shell().to_owned().into();
        let spawner = self.terminal_session_spawner();
        let initial_session =
            self.spawn_initial_session(&configured_shell, &detected_shells, &spawner, cx);
        let owner_hide = cx.weak_entity();
        let owner_empty = cx.weak_entity();
        let owner_cwd = cx.weak_entity();
        let config = TerminalDrawerConfig {
            initial_session,
            detected_shells,
            configured_shell,
            scrollback: DEFAULT_SCROLLBACK,
            spawner,
            cwd_provider: Rc::new(move |cx| {
                owner_cwd
                    .upgrade()
                    .map(|view| view.read(cx).active_local_folder())
                    .unwrap_or_default()
            }),
            on_hide: Rc::new(move |window, cx| {
                let _ = owner_hide.update(cx, |view, cx| view.hide_terminal(window, cx));
            }),
            on_empty: Rc::new(move |window, cx| {
                let _ = owner_empty.update(cx, |view, cx| view.close_empty_terminal(window, cx));
            }),
        };
        let drawer = cx.new(|cx| TerminalDrawer::with_config(config, cx));
        self.terminal_host.drawer = Some(drawer);
    }

    #[cfg(test)]
    fn detected_terminal_shells(&self) -> Vec<ShellSpec> {
        if let Some(shells) = &self.terminal_host.detected_shells {
            return shells.clone();
        }
        vec![ShellSpec {
            kind: navo_term::ShellKind::Other,
            program: PathBuf::from("fake-shell"),
            args: Vec::new(),
            label: String::from("Fake"),
        }]
    }

    #[cfg(not(test))]
    fn detected_terminal_shells(&self) -> Vec<ShellSpec> {
        detect(&Probe::system(Vec::new()))
    }

    #[cfg(test)]
    fn terminal_session_spawner(&self) -> SessionSpawner {
        if let Some(spawner) = &self.terminal_host.session_spawner {
            return Rc::clone(spawner);
        }
        Rc::new(|_, _, _, sink| {
            let (backend, _handle) = navo_term::FakeBackend::new(sink);
            Ok(navo_term::Session::with_backend(Box::new(backend)))
        })
    }

    #[cfg(not(test))]
    fn terminal_session_spawner(&self) -> SessionSpawner {
        crate::terminal::real_spawner()
    }

    fn spawn_initial_session(
        &self,
        configured: &str,
        detected: &[ShellSpec],
        spawner: &SessionSpawner,
        cx: &mut Context<Self>,
    ) -> Entity<TerminalSession> {
        #[cfg(test)]
        if let Some(make) = self.terminal_host.spawner.clone() {
            return cx.new(|cx| make(INITIAL_SIZE, cx));
        }
        let options = SpawnOptions {
            cwd: self.active_local_folder(),
            size: INITIAL_SIZE,
            scrollback: DEFAULT_SCROLLBACK,
            spawner: Rc::clone(spawner),
        };
        spawn_default_session(configured, detected, &options, cx)
    }

    fn active_local_folder(&self) -> PathBuf {
        let folder = self.browser.state().active_tab().location();
        if folder.is_local() {
            return PathBuf::from(folder.path().display().to_string());
        }
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    /// Wraps the main panes body in a vertical [`Splitter`] when the bottom terminal drawer is open.
    pub(super) fn wrap_terminal_drawer(
        &self,
        body: AnyElement,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !self.terminal_host.is_open {
            return body;
        }
        let Some(drawer) = self.terminal_host.drawer.clone() else {
            return body;
        };
        let owner = cx.weak_entity();
        let resize_label = navo_l10n::tr("terminal-drawer-resize");
        let resize_tip = navo_l10n::tr("terminal-drawer-resize-tip");
        Splitter::new(
            "terminal-drawer-split",
            &self.terminal_host.splitter,
            body,
            drawer,
        )
        .vertical()
        .min_second(MIN_DRAWER_HEIGHT)
        .aria_label(resize_label)
        .tooltip(resize_tip)
        .on_commit(move |ratio, window, cx| {
            let pct = ratio_to_drawer_pct(ratio);
            let _ = owner.update(cx, |view, cx| {
                view.persist_terminal_drawer_height(pct, window, cx);
            });
        })
        .into_any_element()
    }

    fn persist_terminal_drawer_height(
        &mut self,
        pct: u16,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.values.terminal_drawer_height() == pct {
            return;
        }
        self.change_setting(
            Change {
                field: Field::TerminalDrawerHeight,
                value: Value::Number(i64::from(pct)),
            },
            window,
            cx,
        );
    }

    /// Renders the module-off notice overlay when `Ctrl+\`` was pressed while the module is off.
    pub(super) fn terminal_off_notice_layer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.terminal_host.off_notice {
            return None;
        }
        let t = tokens::current(cx);
        let turn_on = cx.weak_entity();
        let external = cx.weak_entity();
        let dismiss = cx.weak_entity();
        let notice = TerminalOffNotice::new(
            move |window, cx| {
                let _ = turn_on.update(cx, |view, cx| {
                    view.terminal_host.off_notice = false;
                    view.open_settings(window, cx);
                    cx.notify();
                });
            },
            move |window, cx| {
                let _ = external.update(cx, |view, cx| {
                    view.terminal_host.off_notice = false;
                    view.open_terminal(window, cx);
                    cx.notify();
                });
            },
            move |window, cx| {
                let _ = dismiss.update(cx, |view, cx| {
                    view.terminal_host.off_notice = false;
                    view.focus_core_pane(window, cx);
                    cx.notify();
                });
            },
        );
        Some(
            div()
                .absolute()
                .bottom(t.spacing.md)
                .right(t.spacing.md)
                .child(notice)
                .into_any_element(),
        )
    }

    /// Returns the title-bar Terminal button while `[modules] terminal` is on.
    pub(super) fn title_terminal_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.values.terminal_module_on() {
            return None;
        }
        let owner = cx.weak_entity();
        Some(
            title_bar_terminal_button(
                self.terminal_host.is_open,
                move |window, cx| {
                    let _ = owner.update(cx, |view, cx| view.toggle_terminal_button(window, cx));
                },
                cx,
            )
            .into_any_element(),
        )
    }

    /// Applies updated terminal module, shell, and drawer height settings from `self.values`.
    pub(super) fn sync_terminal_settings(&mut self, cx: &mut App) {
        if !self.values.terminal_module_on() {
            self.terminal_host.shutdown(cx);
        } else {
            self.terminal_host.off_notice = false;
            if let Some(drawer) = &self.terminal_host.drawer {
                let shell: SharedString = self.values.terminal_shell().to_owned().into();
                drawer.update(cx, |drawer, cx| drawer.set_configured_shell(shell, cx));
            }
        }
        self.terminal_host
            .sync_height(self.values.terminal_drawer_height(), cx);
        self.cached.refresh_title(cx);
    }

    #[cfg(test)]
    pub(crate) fn set_terminal_spawner(&mut self, spawner: TestSpawner) {
        self.terminal_host.spawner = Some(spawner);
    }

    #[cfg(test)]
    pub(crate) fn set_terminal_session_spawner(&mut self, spawner: SessionSpawner) {
        self.terminal_host.session_spawner = Some(spawner);
    }

    #[cfg(test)]
    pub(crate) fn set_terminal_detected_shells(&mut self, shells: Vec<ShellSpec>) {
        self.terminal_host.detected_shells = Some(shells);
    }

    #[cfg(test)]
    pub(crate) fn terminal_configured_shell(&self) -> &str {
        self.values.terminal_shell()
    }

    #[cfg(test)]
    pub(crate) fn has_terminal_session(&self) -> bool {
        self.terminal_host.drawer.is_some()
    }

    #[cfg(test)]
    pub(crate) fn is_terminal_open(&self) -> bool {
        self.terminal_host.is_open
    }

    #[cfg(test)]
    pub(crate) fn is_terminal_focused(&self, window: &Window, cx: &App) -> bool {
        self.is_terminal_focused_inner(window, cx)
    }
}
