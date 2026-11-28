//! GPUI model entity wrapping one live [`navo_term::Session`] and its [`navo_term::grid::Grid`].
//!
//! Output from the PTY thread sets the grid's dirty flag and sends on a 1-slot bounded wake
//! channel; a foreground task snapshots once per wake and notifies GPUI views (D161.3, R3).

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui_kit::{App, AppContext as _, Context, Entity, SharedString, Task};
use navo_term::grid::{Grid, Modes, Snapshot, Viewport};
use navo_term::{Backend, PromptState, Session, ShellKind, ShellSpec, Sink, Size, TermError};

use crate::primitives::TabStatus;

/// Monotonic session id counter for stable tab/session identity within a process.
static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

/// Spawner hook creating a [`Session`] for `(shell, cwd, size, sink)`.
pub type SessionSpawner =
    Rc<dyn Fn(&ShellSpec, &Path, Size, Arc<dyn Sink>) -> Result<Session, TermError>>;

/// Returns the production spawner backed by [`Session::spawn`].
pub fn real_spawner() -> SessionSpawner {
    Rc::new(Session::spawn)
}

/// Parameters shared when spawning or falling back a [`TerminalSession`].
#[derive(Clone)]
pub struct SpawnOptions {
    pub cwd: PathBuf,
    pub size: Size,
    pub scrollback: usize,
    pub spawner: SessionSpawner,
}

/// Spawns a session for `configured` against `detected`, returning a missing-shell session if
/// the configured shell is absent or fails to spawn.
pub fn spawn_default_session(
    configured: &str,
    detected: &[ShellSpec],
    options: &SpawnOptions,
    cx: &mut App,
) -> Entity<TerminalSession> {
    match resolve_configured_shell(configured, detected) {
        ShellChoice::Ready(shell) => {
            let fallback = detected
                .iter()
                .find(|candidate| candidate.program != shell.program)
                .cloned();
            spawn_specific_session(shell, fallback, options, cx)
        }
        ShellChoice::Missing { missing, fallback } => cx.new(|cx| {
            TerminalSession::missing_shell(
                missing,
                fallback,
                options.cwd.clone(),
                options.size,
                options.scrollback,
                cx,
            )
        }),
    }
}

/// Spawns an explicitly chosen `shell`, falling back to a missing-shell empty state on failure.
pub fn spawn_specific_session(
    shell: ShellSpec,
    fallback: Option<ShellSpec>,
    options: &SpawnOptions,
    cx: &mut App,
) -> Entity<TerminalSession> {
    let cwd = options.cwd.clone();
    let size = options.size;
    let scrollback = options.scrollback;
    let spawner = Rc::clone(&options.spawner);
    cx.new(
        |cx| match TerminalSession::spawn_with(&shell, &cwd, size, scrollback, &spawner, cx) {
            Ok(session) => session,
            Err(TermError::ShellMissing(missing)) => {
                TerminalSession::missing_shell(missing, fallback, cwd, size, scrollback, cx)
            }
            Err(_) => {
                TerminalSession::missing_shell(shell.label, fallback, cwd, size, scrollback, cx)
            }
        },
    )
}

/// Information displayed when a session's configured shell could not be found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MissingShellInfo {
    /// Configured shell name or path that was not found.
    pub missing: SharedString,
    /// Next detected shell offered on `Enter` without changing the saved setting.
    pub fallback: Option<ShellSpec>,
}

/// Result of resolving a configured `terminal.shell` string against detected shells.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShellChoice {
    /// A matching (or default first detected) shell is ready to spawn.
    Ready(ShellSpec),
    /// The configured shell is missing; offers the next detected shell as fallback.
    Missing {
        missing: String,
        fallback: Option<ShellSpec>,
    },
}

/// Lifecycle state of one [`TerminalSession`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionState {
    Running,
    Exited(Option<u32>),
    MissingShell(MissingShellInfo),
}

/// Resolves the configured shell name (`""` = first detected shell) against `detected`.
#[must_use]
pub fn resolve_configured_shell(configured: &str, detected: &[ShellSpec]) -> ShellChoice {
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        return match detected.first().cloned() {
            Some(spec) => ShellChoice::Ready(spec),
            None => ShellChoice::Missing {
                missing: String::new(),
                fallback: None,
            },
        };
    }
    if let Some(found) = detected
        .iter()
        .find(|spec| matches_configured(spec, trimmed))
    {
        return ShellChoice::Ready(found.clone());
    }
    ShellChoice::Missing {
        missing: trimmed.to_owned(),
        fallback: detected.first().cloned(),
    }
}

/// Returns the index in `detected` matching `configured` (or `0` when blank).
#[must_use]
pub fn default_shell_index(configured: &str, detected: &[ShellSpec]) -> Option<usize> {
    let trimmed = configured.trim();
    if trimmed.is_empty() {
        return (!detected.is_empty()).then_some(0);
    }
    detected
        .iter()
        .position(|spec| matches_configured(spec, trimmed))
}

fn matches_configured(spec: &ShellSpec, configured: &str) -> bool {
    if spec.label.eq_ignore_ascii_case(configured) {
        return true;
    }
    if spec
        .program
        .to_string_lossy()
        .eq_ignore_ascii_case(configured)
    {
        return true;
    }
    let file_name_match = spec
        .program
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(configured));
    let stem_match = spec
        .program
        .file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(configured));
    file_name_match || stem_match || matches_kind_alias(spec.kind, configured)
}

fn matches_kind_alias(kind: ShellKind, configured: &str) -> bool {
    match kind {
        ShellKind::Pwsh => configured.eq_ignore_ascii_case("pwsh"),
        ShellKind::WindowsPowerShell => configured.eq_ignore_ascii_case("powershell"),
        ShellKind::Cmd => configured.eq_ignore_ascii_case("cmd"),
        ShellKind::GitBash => {
            configured.eq_ignore_ascii_case("git-bash")
                || configured.eq_ignore_ascii_case("gitbash")
        }
        ShellKind::Bash => configured.eq_ignore_ascii_case("bash"),
        ShellKind::Zsh => configured.eq_ignore_ascii_case("zsh"),
        ShellKind::Fish => configured.eq_ignore_ascii_case("fish"),
        ShellKind::Wsl | ShellKind::Other => false,
    }
}

/// One terminal session owned by the main window (D161.3).
pub struct TerminalSession {
    id: u64,
    label: SharedString,
    shell: Option<ShellSpec>,
    cwd: PathBuf,
    missing: Option<MissingShellInfo>,
    size: Size,
    viewport: Viewport,
    grid: Arc<Grid>,
    session: Option<Session>,
    snapshot: Arc<Snapshot>,
    _wake_task: Task<()>,
}

struct SessionInit {
    label: SharedString,
    shell: Option<ShellSpec>,
    cwd: PathBuf,
    missing: Option<MissingShellInfo>,
    size: Size,
    grid: Arc<Grid>,
    session: Option<Session>,
}

impl SessionInit {
    fn running(
        shell: ShellSpec,
        cwd: PathBuf,
        size: Size,
        grid: Arc<Grid>,
        session: Session,
    ) -> Self {
        Self {
            label: shell.label.clone().into(),
            shell: Some(shell),
            cwd,
            missing: None,
            size,
            grid,
            session: Some(session),
        }
    }
}

impl TerminalSession {
    /// Spawns a real shell session in `cwd`, creating its GPUI entity only if spawning succeeds.
    pub fn spawn_entity(
        shell: &ShellSpec,
        cwd: &Path,
        size: Size,
        scrollback: usize,
        cx: &mut App,
    ) -> Result<Entity<Self>, TermError> {
        let spawner = real_spawner();
        let (grid, rx) = build_grid(size, scrollback);
        let session = spawner(shell, cwd, size, grid.clone())?;
        let init = SessionInit::running(shell.clone(), cwd.to_path_buf(), size, grid, session);
        Ok(cx.new(|cx| Self::finish(init, rx, cx)))
    }

    /// Spawns a real shell session in `cwd`, waking GPUI on the foreground executor when dirty.
    pub fn spawn(
        shell: &ShellSpec,
        cwd: &Path,
        size: Size,
        scrollback: usize,
        cx: &mut Context<Self>,
    ) -> Result<Self, TermError> {
        Self::spawn_with(shell, cwd, size, scrollback, &real_spawner(), cx)
    }

    /// Spawns a shell session using `spawner` in `cwd`.
    pub fn spawn_with(
        shell: &ShellSpec,
        cwd: &Path,
        size: Size,
        scrollback: usize,
        spawner: &SessionSpawner,
        cx: &mut Context<Self>,
    ) -> Result<Self, TermError> {
        let (grid, rx) = build_grid(size, scrollback);
        let session = spawner(shell, cwd, size, grid.clone())?;
        let init = SessionInit::running(shell.clone(), cwd.to_path_buf(), size, grid, session);
        Ok(Self::finish(init, rx, cx))
    }

    /// Creates a placeholder session in the missing-shell empty state without starting a PTY.
    pub fn missing_shell(
        missing: impl Into<SharedString>,
        fallback: Option<ShellSpec>,
        cwd: PathBuf,
        size: Size,
        scrollback: usize,
        cx: &mut Context<Self>,
    ) -> Self {
        let missing: SharedString = missing.into();
        let label = if missing.is_empty() {
            SharedString::from(navo_l10n::tr("terminal-status-missing"))
        } else {
            missing.clone()
        };
        let (grid, rx) = build_grid(size, scrollback);
        let info = MissingShellInfo { missing, fallback };
        let init = SessionInit {
            label,
            shell: None,
            cwd,
            missing: Some(info),
            size,
            grid,
            session: None,
        };
        Self::finish(init, rx, cx)
    }

    /// Creates a session backed by a caller-supplied backend (for deterministic headless tests).
    pub fn with_backend(
        label: impl Into<SharedString>,
        size: Size,
        scrollback: usize,
        make_backend: impl FnOnce(Arc<dyn Sink>) -> Box<dyn Backend>,
        cx: &mut Context<Self>,
    ) -> Self {
        let label: SharedString = label.into();
        let spec = ShellSpec {
            kind: ShellKind::Other,
            program: PathBuf::from(label.as_ref()),
            args: Vec::new(),
            label: label.to_string(),
        };
        let (grid, rx) = build_grid(size, scrollback);
        let session = Session::with_backend(make_backend(grid.clone()));
        let init = SessionInit::running(spec, PathBuf::new(), size, grid, session);
        Self::finish(init, rx, cx)
    }

    /// Creates a session backed by a [`navo_term::FakeBackend`] for deterministic headless tests.
    #[cfg(test)]
    pub fn with_fake(size: Size, cx: &mut Context<Self>) -> (Self, navo_term::FakeHandle) {
        let mut handle_out = None;
        let session = Self::with_backend(
            "Fake",
            size,
            navo_term::grid::DEFAULT_SCROLLBACK,
            |sink| {
                let (backend, handle) = navo_term::FakeBackend::new(sink);
                handle_out = Some(handle);
                Box::new(backend)
            },
            cx,
        );
        let handle = handle_out.expect("make_backend runs synchronously");
        (session, handle)
    }

    fn finish(init: SessionInit, mut rx: mpsc::Receiver<()>, cx: &mut Context<Self>) -> Self {
        let viewport = Viewport::default();
        let snapshot = Arc::new(init.grid.snapshot(viewport));
        let wake_task =
            cx.spawn(
                async move |this, cx| {
                    while rx.next().await.is_some() && this.update(cx, Self::on_wake).is_ok() {}
                },
            );
        Self {
            id: NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed),
            label: init.label,
            shell: init.shell,
            cwd: init.cwd,
            missing: init.missing,
            size: init.size,
            viewport,
            grid: init.grid,
            session: init.session,
            snapshot,
            _wake_task: wake_task,
        }
    }

    fn on_wake(&mut self, cx: &mut Context<Self>) {
        if self.grid.is_dirty() {
            self.snapshot = Arc::new(self.grid.snapshot(self.viewport));
            cx.notify();
        }
    }

    /// Stable process-local identifier for this session.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Latest grid snapshot for painting.
    pub fn snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&self.snapshot)
    }

    /// Active terminal input modes (application cursor, bracketed paste).
    pub fn modes(&self) -> Modes {
        self.grid.modes()
    }

    /// Process exit status once the shell ends (`Some(None)` if status was unavailable).
    pub fn exit_code(&self) -> Option<Option<u32>> {
        self.grid.exit()
    }

    /// Current session lifecycle state.
    pub fn state(&self) -> SessionState {
        if let Some(info) = &self.missing {
            return SessionState::MissingShell(info.clone());
        }
        if let Some(code) = self.grid.exit() {
            return SessionState::Exited(code);
        }
        SessionState::Running
    }

    /// Optional 14 px tab status indicator for this session.
    pub fn tab_status(&self) -> Option<TabStatus> {
        match self.state() {
            SessionState::MissingShell(_) => Some(TabStatus::Warning),
            SessionState::Exited(_) => Some(TabStatus::Exited),
            SessionState::Running => {
                let session = self.session.as_ref()?;
                if matches!(session.prompt(), PromptState::Running { .. }) {
                    Some(TabStatus::Dot)
                } else if session.last_exit_code().is_some_and(|code| code != 0) {
                    Some(TabStatus::Failed)
                } else {
                    None
                }
            }
        }
    }

    /// Current terminal size in cells.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Human-readable shell label.
    pub fn label(&self) -> &SharedString {
        &self.label
    }

    /// Working folder this session started in.
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Shell specification currently running (or last run) in this session.
    pub fn shell(&self) -> Option<&ShellSpec> {
        self.shell.as_ref()
    }

    /// Compact tab title combining shell label and working folder leaf name.
    pub fn tab_label(&self) -> SharedString {
        let folder = folder_leaf(&self.cwd);
        match folder {
            Some(leaf) if !leaf.is_empty() => format!("{} · {leaf}", self.label).into(),
            _ => self.label.clone(),
        }
    }

    /// Restarts an exited session in its original folder, keeping scrollback and inserting a muted
    /// separator row.
    pub fn restart(
        &mut self,
        spawner: &SessionSpawner,
        cx: &mut Context<Self>,
    ) -> Result<(), TermError> {
        let Some(shell) = self.shell.clone() else {
            return Err(TermError::ShellMissing(self.label.to_string()));
        };
        self.session.take();
        self.grid.clear_exit();
        let separator = navo_l10n::tr("terminal-restart-separator");
        let banner = format!("\r\n\x1b[2m{separator}\x1b[0m\r\n");
        self.grid.output(banner.as_bytes());
        let next = spawner(&shell, &self.cwd, self.size, self.grid.clone())?;
        self.session = Some(next);
        self.missing = None;
        self.snapshot = Arc::new(self.grid.snapshot(self.viewport));
        cx.notify();
        Ok(())
    }

    /// Starts `fallback` inside a missing-shell session without mutating any saved setting.
    pub fn launch_fallback(
        &mut self,
        fallback: ShellSpec,
        spawner: &SessionSpawner,
        cx: &mut Context<Self>,
    ) -> Result<(), TermError> {
        self.session.take();
        self.grid.clear_exit();
        let next = spawner(&fallback, &self.cwd, self.size, self.grid.clone())?;
        self.label = fallback.label.clone().into();
        self.shell = Some(fallback);
        self.missing = None;
        self.session = Some(next);
        self.snapshot = Arc::new(self.grid.snapshot(self.viewport));
        cx.notify();
        Ok(())
    }

    /// Sends raw input bytes to the terminal backend.
    pub fn send_bytes(&mut self, bytes: &[u8]) -> Result<(), TermError> {
        if bytes.is_empty() {
            return Ok(());
        }
        let session = self.session.as_mut().ok_or(TermError::Exited)?;
        session.write(bytes)
    }

    /// Resizes the grid and backend, ignoring zero dimensions or unchanged sizes (R4).
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), TermError> {
        if cols == 0 || rows == 0 {
            return Ok(());
        }
        let next = Size { cols, rows };
        if next == self.size {
            return Ok(());
        }
        self.grid.resize(next);
        self.size = next;
        if let Some(session) = self.session.as_mut() {
            session.resize(next)?;
        }
        self.snapshot = Arc::new(self.grid.snapshot(self.viewport));
        Ok(())
    }

    /// Ends the session cleanly without blocking the UI thread.
    pub fn shutdown(&mut self) {
        self.session.take();
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn folder_leaf(path: &Path) -> Option<String> {
    if path.as_os_str().is_empty() {
        return None;
    }
    if let Some(name) = path.file_name() {
        let text = name.to_string_lossy();
        if !text.is_empty() {
            return Some(text.into_owned());
        }
    }
    let display = path.display().to_string();
    (!display.is_empty()).then_some(display)
}

fn build_grid(size: Size, scrollback: usize) -> (Arc<Grid>, mpsc::Receiver<()>) {
    let (tx, rx) = mpsc::channel::<()>(1);
    let tx = Mutex::new(tx);
    let grid = Arc::new(Grid::new(size, scrollback, move || {
        let mut sender = tx.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = sender.try_send(());
    }));
    (grid, rx)
}
