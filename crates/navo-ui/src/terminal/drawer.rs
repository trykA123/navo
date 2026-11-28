//! Bottom terminal drawer view managing multiple [`TerminalSession`] tabs, shell picker,
//! responsive header, exited-session restart, and missing-shell fallback.

mod view;

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{FocusHandle, KeyDownEvent, Keystroke, Subscription, Task};
use navo_term::grid::DEFAULT_SCROLLBACK;
use navo_term::{ShellSpec, Size};

use super::RowCache;
use super::input::{encode_key, encode_paste, is_toggle_shortcut};
use super::session::{
    SessionSpawner, SessionState, SpawnOptions, TerminalSession, real_spawner,
    spawn_default_session, spawn_specific_session,
};
pub use super::states::{TerminalOffNotice, title_bar_terminal_button};
use crate::prelude::*;
use crate::primitives::popup_menu::{self, PopupMenu};

/// Default bottom drawer height as a percentage of the window height.
pub const DEFAULT_DRAWER_HEIGHT_PCT: u16 = 40;
/// Minimum bottom drawer height in logical pixels.
pub const MIN_DRAWER_HEIGHT: Pixels = px(168.0);
/// Keymap context for a focused terminal view.
pub const CONTEXT: &str = "Terminal";

/// Debounce delay before sending a PTY resize after layout bounds settle (R4).
const RESIZE_DEBOUNCE: Duration = Duration::from_millis(50);

gpui_kit::actions!(
    terminal,
    [
        NewSession,
        OpenShellPicker,
        CloseSession,
        NextSession,
        PreviousSession
    ]
);

/// Registers built-in keybindings for the `terminal` action namespace.
pub(crate) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "terminal");
}

type Callback = Rc<dyn Fn(&mut Window, &mut App)>;
type CwdProvider = Rc<dyn Fn(&App) -> PathBuf>;

/// Configuration for building a multi-session [`TerminalDrawer`].
pub struct TerminalDrawerConfig {
    pub initial_session: Entity<TerminalSession>,
    pub detected_shells: Vec<ShellSpec>,
    pub configured_shell: SharedString,
    pub scrollback: usize,
    pub spawner: SessionSpawner,
    pub cwd_provider: CwdProvider,
    pub on_hide: Callback,
    pub on_empty: Callback,
}

struct SessionEntry {
    entity: Entity<TerminalSession>,
    cache: Entity<RowCache>,
    _sub: Subscription,
}

/// Bottom terminal drawer view managing one or more [`TerminalSession`] entities (D161.3).
pub struct TerminalDrawer {
    sessions: Vec<SessionEntry>,
    active: usize,
    detected_shells: Vec<ShellSpec>,
    configured_shell: SharedString,
    scrollback: usize,
    spawner: SessionSpawner,
    cwd_provider: CwdProvider,
    shell_picker: Entity<PopupMenu>,
    tabs_menu: Entity<PopupMenu>,
    _picker_sub: Subscription,
    _tabs_menu_sub: Subscription,
    focus: FocusHandle,
    measured_width: Rc<Cell<Option<Pixels>>>,
    measured_dims: Rc<Cell<(u16, u16)>>,
    pending_dims: (u16, u16),
    resize_task: Option<Task<()>>,
    on_hide: Callback,
    on_empty: Callback,
}

impl TerminalDrawer {
    /// Creates a drawer view observing a single `session` with default configuration.
    pub fn new(
        session: Entity<TerminalSession>,
        on_hide: impl Fn(&mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> Self {
        let on_hide: Callback = Rc::new(on_hide);
        let on_empty = Rc::clone(&on_hide);
        let detected_shells = session.read(cx).shell().cloned().into_iter().collect();
        let config = TerminalDrawerConfig {
            initial_session: session,
            detected_shells,
            configured_shell: SharedString::default(),
            scrollback: DEFAULT_SCROLLBACK,
            spawner: real_spawner(),
            cwd_provider: Rc::new(|_| PathBuf::new()),
            on_hide,
            on_empty,
        };
        Self::with_config(config, cx)
    }

    /// Creates a multi-session drawer view wired to shell detection, spawner, and callbacks.
    pub fn with_config(config: TerminalDrawerConfig, cx: &mut Context<Self>) -> Self {
        let initial = config.initial_session.read(cx).size();
        let focus = cx.focus_handle();
        let shell_picker = cx.new(|cx| PopupMenu::new(cx));
        let tabs_menu = cx.new(|cx| PopupMenu::new(cx));
        let picker_sub = cx.observe(&shell_picker, |_, _, cx| cx.notify());
        let tabs_menu_sub = cx.observe(&tabs_menu, |_, _, cx| cx.notify());
        let first = make_entry(config.initial_session, cx);
        Self {
            sessions: vec![first],
            active: 0,
            detected_shells: config.detected_shells,
            configured_shell: config.configured_shell,
            scrollback: config.scrollback,
            spawner: config.spawner,
            cwd_provider: config.cwd_provider,
            shell_picker,
            tabs_menu,
            _picker_sub: picker_sub,
            _tabs_menu_sub: tabs_menu_sub,
            focus,
            measured_width: Rc::new(Cell::new(None)),
            measured_dims: Rc::new(Cell::new((initial.cols, initial.rows))),
            pending_dims: (initial.cols, initial.rows),
            resize_task: None,
            on_hide: config.on_hide,
            on_empty: config.on_empty,
        }
    }

    /// Focus handle for the drawer's interactive terminal surface.
    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    /// Returns whether the drawer currently holds keyboard focus in `window`.
    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    /// Number of open sessions in this drawer.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Index of the currently active session tab.
    pub fn active_index(&self) -> usize {
        self.active
    }

    /// Currently active [`TerminalSession`] entity, if any.
    pub fn active_session(&self) -> Option<Entity<TerminalSession>> {
        self.sessions.get(self.active).map(|e| e.entity.clone())
    }

    /// Updates the configured default shell string (without mutating existing sessions).
    pub fn set_configured_shell(&mut self, shell: SharedString, cx: &mut Context<Self>) {
        if self.configured_shell != shell {
            self.configured_shell = shell;
            cx.notify();
        }
    }

    /// Shuts down every open session cleanly without blocking the UI thread.
    pub fn shutdown_all(&mut self, cx: &mut App) {
        for entry in self.sessions.drain(..) {
            entry.entity.update(cx, |session, _| session.shutdown());
        }
        self.active = 0;
    }

    /// Spawns a new session with the configured (or first detected) shell and activates it.
    pub fn new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let options = self.spawn_options(cx);
        let entity =
            spawn_default_session(&self.configured_shell, &self.detected_shells, &options, cx);
        self.push_session(entity, window, cx);
    }

    /// Spawns a new session using an explicitly picked [`ShellSpec`] and activates it.
    pub fn new_session_with_shell(
        &mut self,
        shell: ShellSpec,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let options = self.spawn_options(cx);
        let fallback = self
            .detected_shells
            .iter()
            .find(|candidate| candidate.program != shell.program)
            .cloned();
        let entity = spawn_specific_session(shell, fallback, &options, cx);
        self.push_session(entity, window, cx);
    }

    fn spawn_options(&self, cx: &App) -> SpawnOptions {
        SpawnOptions {
            cwd: (self.cwd_provider)(cx),
            size: current_size(&self.measured_dims),
            scrollback: self.scrollback,
            spawner: Rc::clone(&self.spawner),
        }
    }

    /// Opens the shell picker popup menu if at least one shell was detected.
    pub fn open_shell_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.detected_shells.is_empty() {
            return;
        }
        popup_menu::open(&self.shell_picker, window, cx);
        cx.notify();
    }

    /// Activates session tab `index` and focuses the terminal surface.
    pub fn select_session(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.sessions.len() {
            return;
        }
        self.active = index;
        self.apply_current_dims_to_active(cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Cycles forward to the next session tab.
    pub fn next_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sessions.len() <= 1 {
            return;
        }
        let next = (self.active + 1) % self.sessions.len();
        self.select_session(next, window, cx);
    }

    /// Cycles backward to the previous session tab.
    pub fn previous_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let len = self.sessions.len();
        if len <= 1 {
            return;
        }
        let prev = (self.active + len - 1) % len;
        self.select_session(prev, window, cx);
    }

    /// Closes session `index`; invokes `on_empty` if it was the last session.
    pub fn close_session_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.sessions.len() {
            return;
        }
        let removed = self.sessions.remove(index);
        removed.entity.update(cx, |session, _| session.shutdown());
        if self.sessions.is_empty() {
            self.active = 0;
            let on_empty = Rc::clone(&self.on_empty);
            on_empty(window, cx);
            cx.notify();
            return;
        }
        if index < self.active || self.active >= self.sessions.len() {
            self.active = self.active.saturating_sub(1).min(self.sessions.len() - 1);
        }
        self.apply_current_dims_to_active(cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Restarts the active exited session in its original folder while keeping scrollback.
    pub fn restart_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.sessions.get(self.active) else {
            return;
        };
        let spawner = Rc::clone(&self.spawner);
        let entity = entry.entity.clone();
        let _ = entity.update(cx, |session, cx| session.restart(&spawner, cx));
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Starts the fallback shell offered by the active missing-shell session without mutating
    /// any saved setting.
    pub fn start_fallback_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.sessions.get(self.active) else {
            return;
        };
        let SessionState::MissingShell(info) = entry.entity.read(cx).state() else {
            return;
        };
        let Some(fallback) = info.fallback else {
            return;
        };
        let spawner = Rc::clone(&self.spawner);
        let entity = entry.entity.clone();
        let _ = entity.update(cx, |session, cx| {
            session.launch_fallback(fallback, &spawner, cx)
        });
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn push_session(
        &mut self,
        entity: Entity<TerminalSession>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entry = make_entry(entity, cx);
        self.sessions.push(entry);
        self.active = self.sessions.len() - 1;
        self.apply_current_dims_to_active(cx);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn apply_current_dims_to_active(&self, cx: &mut Context<Self>) {
        let dims = self.measured_dims.get();
        if dims.0 == 0 || dims.1 == 0 {
            return;
        }
        if let Some(entry) = self.sessions.get(self.active) {
            entry
                .entity
                .update(cx, |session, cx| apply_resize(session, dims, cx));
        }
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if is_session_shortcut(&event.keystroke) || is_toggle_shortcut(&event.keystroke) {
            return;
        }
        let Some(active) = self.active_session() else {
            return;
        };
        match active.read(cx).state() {
            SessionState::MissingShell(_) => {
                if is_plain_enter(&event.keystroke) {
                    self.start_fallback_active(window, cx);
                    cx.stop_propagation();
                }
            }
            SessionState::Exited(_) => {
                if is_plain_enter(&event.keystroke) {
                    self.restart_active(window, cx);
                    cx.stop_propagation();
                }
            }
            SessionState::Running => self.handle_running_key(&active, event, cx),
        }
    }

    fn handle_running_key(
        &self,
        active: &Entity<TerminalSession>,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        if is_paste_shortcut(&event.keystroke) {
            self.paste_clipboard(active, cx);
            cx.stop_propagation();
            return;
        }
        let modes = active.read(cx).modes();
        let Some(bytes) = encode_key(&event.keystroke, modes) else {
            return;
        };
        active.update(cx, |session, _| {
            let _ = session.send_bytes(&bytes);
        });
        cx.stop_propagation();
    }

    fn paste_clipboard(&self, active: &Entity<TerminalSession>, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        let modes = active.read(cx).modes();
        let bytes = encode_paste(&text, modes);
        active.update(cx, |session, _| {
            let _ = session.send_bytes(&bytes);
        });
    }

    fn sync_measured_size(&mut self, cx: &mut Context<Self>) {
        let dims = self.measured_dims.get();
        if dims.0 == 0 || dims.1 == 0 || dims == self.pending_dims {
            return;
        }
        self.pending_dims = dims;
        let Some(session) = self.active_session() else {
            return;
        };
        self.resize_task = Some(cx.spawn(async move |_, cx| {
            cx.background_executor().timer(RESIZE_DEBOUNCE).await;
            session.update(cx, |session, cx| apply_resize(session, dims, cx));
        }));
    }
}

fn make_entry(entity: Entity<TerminalSession>, cx: &mut Context<TerminalDrawer>) -> SessionEntry {
    let cache = cx.new(|_| RowCache::new());
    let sub = cx.observe(&entity, |_, _, cx| cx.notify());
    SessionEntry {
        entity,
        cache,
        _sub: sub,
    }
}

fn current_size(measured: &Rc<Cell<(u16, u16)>>) -> Size {
    let (cols, rows) = measured.get();
    Size {
        cols: if cols == 0 { 80 } else { cols },
        rows: if rows == 0 { 24 } else { rows },
    }
}

fn apply_resize(
    session: &mut TerminalSession,
    dims: (u16, u16),
    cx: &mut Context<TerminalSession>,
) {
    if session.resize(dims.0, dims.1).is_ok() {
        cx.notify();
    }
}

fn is_paste_shortcut(keystroke: &Keystroke) -> bool {
    let m = keystroke.modifiers;
    keystroke.key.eq_ignore_ascii_case("v") && m.control && m.shift && !m.alt && !m.platform
}

fn is_plain_enter(keystroke: &Keystroke) -> bool {
    let m = keystroke.modifiers;
    keystroke.key == "enter" && !m.control && !m.alt && !m.shift && !m.platform
}

fn is_session_shortcut(keystroke: &Keystroke) -> bool {
    let m = keystroke.modifiers;
    if !m.control || m.alt || m.platform {
        return false;
    }
    if keystroke.key == "tab" {
        return true;
    }
    m.shift
        && (keystroke.key.eq_ignore_ascii_case("t")
            || keystroke.key.eq_ignore_ascii_case("w")
            || keystroke.key == "space")
}
