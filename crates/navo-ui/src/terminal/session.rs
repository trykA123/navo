//! GPUI model entity wrapping one live [`navo_term::Session`] and its [`navo_term::grid::Grid`].
//!
//! Output from the PTY thread sets the grid's dirty flag and sends on a 1-slot bounded wake
//! channel; a foreground task snapshots once per wake and notifies GPUI views (D161.3, R3).

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use futures::StreamExt as _;
use futures::channel::mpsc;
use gpui_kit::{App, AppContext as _, Context, Entity, SharedString, Task};
use navo_term::grid::{Grid, Modes, Snapshot, Viewport};
use navo_term::{Backend, Session, ShellSpec, Sink, Size, TermError};

/// One terminal session owned by the main window (D161.3).
pub struct TerminalSession {
    label: SharedString,
    size: Size,
    viewport: Viewport,
    grid: Arc<Grid>,
    session: Option<Session>,
    snapshot: Arc<Snapshot>,
    _wake_task: Task<()>,
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
        let (grid, rx) = build_grid(size, scrollback);
        let sink: Arc<dyn Sink> = grid.clone();
        let session = Session::spawn(shell, cwd, size, sink)?;
        let label: SharedString = shell.label.clone().into();
        Ok(cx.new(|cx| Self::finish(label, size, grid, session, rx, cx)))
    }

    /// Spawns a real shell session in `cwd`, waking GPUI on the foreground executor when dirty.
    pub fn spawn(
        shell: &ShellSpec,
        cwd: &Path,
        size: Size,
        scrollback: usize,
        cx: &mut Context<Self>,
    ) -> Result<Self, TermError> {
        let (grid, rx) = build_grid(size, scrollback);
        let sink: Arc<dyn Sink> = grid.clone();
        let session = Session::spawn(shell, cwd, size, sink)?;
        Ok(Self::finish(
            shell.label.clone().into(),
            size,
            grid,
            session,
            rx,
            cx,
        ))
    }

    /// Creates a session backed by a caller-supplied backend (for deterministic headless tests).
    pub fn with_backend(
        label: impl Into<SharedString>,
        size: Size,
        scrollback: usize,
        make_backend: impl FnOnce(Arc<dyn Sink>) -> Box<dyn Backend>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (grid, rx) = build_grid(size, scrollback);
        let sink: Arc<dyn Sink> = grid.clone();
        let session = Session::with_backend(make_backend(sink));
        Self::finish(label.into(), size, grid, session, rx, cx)
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

    fn finish(
        label: SharedString,
        size: Size,
        grid: Arc<Grid>,
        session: Session,
        mut rx: mpsc::Receiver<()>,
        cx: &mut Context<Self>,
    ) -> Self {
        let viewport = Viewport::default();
        let snapshot = Arc::new(grid.snapshot(viewport));
        let wake_task =
            cx.spawn(
                async move |this, cx| {
                    while rx.next().await.is_some() && this.update(cx, Self::on_wake).is_ok() {}
                },
            );
        Self {
            label,
            size,
            viewport,
            grid,
            session: Some(session),
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

    /// Current terminal size in cells.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Human-readable shell label.
    pub fn label(&self) -> &SharedString {
        &self.label
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

fn build_grid(size: Size, scrollback: usize) -> (Arc<Grid>, mpsc::Receiver<()>) {
    let (tx, rx) = mpsc::channel::<()>(1);
    let tx = Mutex::new(tx);
    let grid = Arc::new(Grid::new(size, scrollback, move || {
        let mut sender = tx.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = sender.try_send(());
    }));
    (grid, rx)
}
