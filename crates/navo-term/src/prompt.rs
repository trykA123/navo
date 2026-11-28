//! Trusted OSC 133 prompt readiness, one serialized PTY writer and manual folder sync.
//! Every write to the PTY goes through `PtyWriter` under the same mutex as `PromptState`, so
//! user keystrokes, manual sync and automatic pending-sync retries can never interleave.

mod nonce;
mod osc133;
mod shell;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Weak};
use std::time::Instant;

use parking_lot::Mutex;

use crate::{Backend, ShellKind, Sink, Size, TermError};
use osc133::{MAX_COMMAND_LEN, Osc133Event, Osc133Parser};

pub use nonce::Nonce;
pub use shell::{ShellHook, quote_sync_command};

/// Bounded ring of OSC 133 marks kept for command blocks (NV13h).
const MAX_MARKS: usize = 4_096;

/// Whether the shell is at a trusted idle prompt, busy running a command, or unintegrated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptState {
    /// No trusted `133;A;navo=<nonce>` has arrived yet; readiness cannot be trusted.
    Unknown,
    /// A trusted `133;A` arrived; `typed` turns `true` as soon as the user sends input.
    AtPrompt { typed: bool },
    /// A command is running (user pressed Enter, sync wrote a `cd`, or `133;C`/`133;D` arrived).
    Running {
        command: String,
        started_at: Instant,
    },
}

/// Why manual folder sync cannot write right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncWait {
    /// A command is running; holds the command text when known.
    Running(String),
    /// The user has typed at the current prompt.
    Typed,
    /// The shell has not emitted a trusted prompt marker.
    NeedsIntegration,
    /// The target folder path cannot be safely quoted for this shell.
    UnusualName,
}

/// Which OSC 133 lifecycle mark arrived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkKind {
    /// Trusted `133;A`: start of an idle prompt.
    PromptStart,
    /// `133;B`: prompt end / start of typed command.
    CommandStart,
    /// `133;C`: command execution began.
    CommandExecuted { command: Option<String> },
    /// `133;D`: command finished, carrying its optional exit code.
    CommandFinished { exit_code: Option<i32> },
}

/// One recorded OSC 133 mark, anchored to its grid line for command blocks (NV13h).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptMark {
    pub kind: MarkKind,
    pub line: Option<usize>,
    pub at: Instant,
}

struct WriterInner {
    backend: Box<dyn Backend>,
    shell_kind: ShellKind,
    nonce: Option<Nonce>,
    state: PromptState,
    pending_sync: Option<PathBuf>,
    typed_line: String,
    typed_line_dirty: bool,
    marks: VecDeque<PromptMark>,
    last_exit_code: Option<i32>,
}

impl WriterInner {
    fn write_user(&mut self, bytes: &[u8]) -> Result<(), TermError> {
        if bytes.is_empty() {
            return Ok(());
        }
        // Update prompt state before bytes reach the PTY so sync never races typed input.
        if matches!(self.state, PromptState::AtPrompt { .. }) {
            self.advance_state_on_user_bytes(bytes);
        }
        self.backend.write(bytes)
    }

    fn advance_state_on_user_bytes(&mut self, bytes: &[u8]) {
        if let Some(enter_at) = bytes
            .iter()
            .position(|&byte| byte == b'\r' || byte == b'\n')
        {
            self.record_typed_preview(bytes.get(..enter_at).unwrap_or_default());
            let command = if self.typed_line_dirty {
                String::new()
            } else {
                self.typed_line.trim().to_owned()
            };
            self.clear_typed_preview();
            self.state = PromptState::Running {
                command,
                started_at: Instant::now(),
            };
        } else {
            self.record_typed_preview(bytes);
            self.state = PromptState::AtPrompt { typed: true };
        }
    }

    /// Best-effort preview of what the user typed at `AtPrompt` for `SyncWait::Running(cmd)`.
    /// Never clears `typed` (fail-closed) and never runs while `Running` (passwords).
    fn record_typed_preview(&mut self, bytes: &[u8]) {
        if self.typed_line_dirty {
            return;
        }
        let Ok(text) = std::str::from_utf8(bytes) else {
            self.mark_typed_dirty();
            return;
        };
        for ch in text.chars() {
            match ch {
                '\x08' | '\x7f' => {
                    self.typed_line.pop();
                }
                ch if ch.is_control() => {
                    self.mark_typed_dirty();
                    return;
                }
                ch if self.typed_line.len().saturating_add(ch.len_utf8()) <= MAX_COMMAND_LEN => {
                    self.typed_line.push(ch);
                }
                _ => {}
            }
        }
    }

    fn mark_typed_dirty(&mut self) {
        self.typed_line.clear();
        self.typed_line_dirty = true;
    }

    fn clear_typed_preview(&mut self) {
        self.typed_line.clear();
        self.typed_line_dirty = false;
    }

    fn sync_cwd(&mut self, folder: &Path) -> Result<Option<SyncWait>, TermError> {
        if matches!(self.state, PromptState::Unknown) {
            self.pending_sync = None;
            return Ok(Some(SyncWait::NeedsIntegration));
        }
        let command = match quote_sync_command(self.shell_kind, folder) {
            Ok(command) => command,
            Err(wait) => {
                // Clear any older pending folder so a superseded folder is never synced later.
                self.pending_sync = None;
                return Ok(Some(wait));
            }
        };
        match &self.state {
            PromptState::AtPrompt { typed: false } => {
                self.pending_sync = None;
                self.write_sync_command(&command)?;
                Ok(None)
            }
            PromptState::AtPrompt { typed: true } => {
                self.pending_sync = Some(folder.to_path_buf());
                Ok(Some(SyncWait::Typed))
            }
            PromptState::Running { command, .. } => {
                let reason = SyncWait::Running(command.clone());
                self.pending_sync = Some(folder.to_path_buf());
                Ok(Some(reason))
            }
            PromptState::Unknown => Ok(Some(SyncWait::NeedsIntegration)),
        }
    }

    fn write_sync_command(&mut self, command: &str) -> Result<(), TermError> {
        match self.backend.write(command.as_bytes()) {
            Ok(()) => {
                self.clear_typed_preview();
                let display = command.trim_end_matches(['\r', '\n']).to_owned();
                self.state = PromptState::Running {
                    command: display,
                    started_at: Instant::now(),
                };
                Ok(())
            }
            // InputFull/Exited sent zero bytes; leave state clean.
            Err(TermError::InputFull) => Err(TermError::InputFull),
            Err(TermError::Exited) => Err(TermError::Exited),
            // Any other write error may have delivered partial bytes: fail closed.
            Err(error) => {
                self.state = PromptState::AtPrompt { typed: true };
                Err(error)
            }
        }
    }

    fn sync_reason(&self, folder: &Path) -> Option<SyncWait> {
        if matches!(self.state, PromptState::Unknown) {
            return Some(SyncWait::NeedsIntegration);
        }
        if let Err(wait) = quote_sync_command(self.shell_kind, folder) {
            return Some(wait);
        }
        match &self.state {
            PromptState::Unknown => Some(SyncWait::NeedsIntegration),
            PromptState::AtPrompt { typed: false } => None,
            PromptState::AtPrompt { typed: true } => Some(SyncWait::Typed),
            PromptState::Running { command, .. } => Some(SyncWait::Running(command.clone())),
        }
    }

    fn handle_osc133(&mut self, event: Osc133Event, line: Option<usize>) {
        let now = Instant::now();
        match event {
            Osc133Event::PromptStart { navo_nonce } => self.on_prompt_start(navo_nonce, line, now),
            Osc133Event::CommandStart => {
                if !matches!(self.state, PromptState::Unknown) {
                    self.record_mark(MarkKind::CommandStart, line, now);
                }
            }
            Osc133Event::CommandExecuted { command } => {
                self.on_command_executed(command, line, now);
            }
            Osc133Event::CommandFinished { exit_code } => {
                self.on_command_finished(exit_code, line, now);
            }
        }
    }

    fn on_prompt_start(&mut self, navo_nonce: Option<String>, line: Option<usize>, at: Instant) {
        let trusted = self
            .nonce
            .as_ref()
            .zip(navo_nonce.as_deref())
            .is_some_and(|(expected, actual)| expected.matches(actual));
        if !trusted {
            return;
        }
        self.record_mark(MarkKind::PromptStart, line, at);
        self.clear_typed_preview();
        self.state = PromptState::AtPrompt { typed: false };
        // Flush pending folder while still holding the writer lock so no keystroke can interleave.
        if let Some(target) = self.pending_sync.take()
            && let Ok(command) = quote_sync_command(self.shell_kind, &target)
            && let Err(error) = self.write_sync_command(&command)
        {
            tracing::warn!(%error, "pending terminal folder sync failed");
        }
    }

    fn on_command_executed(&mut self, command: Option<String>, line: Option<usize>, at: Instant) {
        if matches!(self.state, PromptState::Unknown) {
            return;
        }
        self.record_mark(
            MarkKind::CommandExecuted {
                command: command.clone(),
            },
            line,
            at,
        );
        let text = command.unwrap_or_else(|| match &self.state {
            PromptState::Running { command, .. } => command.clone(),
            _ if !self.typed_line_dirty => self.typed_line.trim().to_owned(),
            _ => String::new(),
        });
        self.clear_typed_preview();
        self.state = PromptState::Running {
            command: text,
            started_at: at,
        };
    }

    fn on_command_finished(&mut self, exit_code: Option<i32>, line: Option<usize>, at: Instant) {
        if matches!(self.state, PromptState::Unknown) {
            return;
        }
        if exit_code.is_some() {
            self.last_exit_code = exit_code;
        }
        self.record_mark(MarkKind::CommandFinished { exit_code }, line, at);
        if !matches!(self.state, PromptState::Running { .. }) {
            self.clear_typed_preview();
            self.state = PromptState::Running {
                command: String::new(),
                started_at: at,
            };
        }
    }

    fn record_mark(&mut self, kind: MarkKind, line: Option<usize>, at: Instant) {
        if self.marks.len() >= MAX_MARKS {
            self.marks.pop_front();
        }
        self.marks.push_back(PromptMark { kind, line, at });
    }
}

/// The single serialized writer for a session: protects prompt state and backend writes under
/// one mutex so user input, manual sync and trusted `133;A` retries never interleave.
pub struct PtyWriter {
    inner: Mutex<WriterInner>,
}

impl PtyWriter {
    /// Creates a writer over `backend` for `shell_kind`; `nonce` is `None` when unintegrated.
    pub fn new(shell_kind: ShellKind, nonce: Option<Nonce>, backend: Box<dyn Backend>) -> Self {
        Self {
            inner: Mutex::new(WriterInner {
                backend,
                shell_kind,
                nonce,
                state: PromptState::Unknown,
                pending_sync: None,
                typed_line: String::new(),
                typed_line_dirty: false,
                marks: VecDeque::new(),
                last_exit_code: None,
            }),
        }
    }

    /// Writes user bytes atomically after updating prompt state under the writer lock.
    pub fn write_user(&self, bytes: &[u8]) -> Result<(), TermError> {
        self.inner.lock().write_user(bytes)
    }

    /// Resizes the underlying backend through the same serialized owner.
    pub fn resize(&self, size: Size) -> Result<(), TermError> {
        self.inner.lock().backend.resize(size)
    }

    /// Requests a manual folder sync; writes immediately only in `AtPrompt { typed: false }`.
    pub fn sync_cwd(&self, folder: &Path) -> Result<Option<SyncWait>, TermError> {
        self.inner.lock().sync_cwd(folder)
    }

    /// Why syncing to `folder` would wait right now, or `None` if it can write immediately.
    pub fn sync_reason(&self, folder: &Path) -> Option<SyncWait> {
        self.inner.lock().sync_reason(folder)
    }

    /// Current prompt state snapshot.
    pub fn prompt_state(&self) -> PromptState {
        self.inner.lock().state.clone()
    }

    /// Pending target folder queued for the next trusted `133;A`, if any.
    pub fn pending_sync(&self) -> Option<PathBuf> {
        self.inner.lock().pending_sync.clone()
    }

    /// Recorded OSC 133 marks for command blocks (NV13h).
    pub fn marks(&self) -> Vec<PromptMark> {
        self.inner.lock().marks.iter().cloned().collect()
    }

    /// Exit code reported by the latest `133;D`, if any.
    pub fn last_exit_code(&self) -> Option<i32> {
        self.inner.lock().last_exit_code
    }

    fn handle_osc133(&self, event: Osc133Event, line: Option<usize>) {
        self.inner.lock().handle_osc133(event, line);
    }

    fn on_exited(&self) {
        self.inner.lock().pending_sync = None;
    }
}

/// Output `Sink` wrapper that scans OSC 133 marks via `vte` before forwarding bytes downstream.
pub struct PromptSink {
    writer: Weak<PtyWriter>,
    inner: Arc<dyn Sink>,
    parser: Mutex<Osc133Parser>,
}

impl PromptSink {
    /// Wraps `inner` to feed OSC 133 events into `writer` without creating a reference cycle.
    pub fn for_writer(writer: &Arc<PtyWriter>, inner: Arc<dyn Sink>) -> Self {
        Self {
            writer: Arc::downgrade(writer),
            inner,
            parser: Mutex::new(Osc133Parser::new()),
        }
    }
}

impl Sink for PromptSink {
    fn output(&self, bytes: &[u8]) {
        let mut parser = self.parser.lock();
        let mut rest = bytes;
        while !rest.is_empty() {
            let (slice, event) = parser.advance(rest);
            let consumed = slice.len();
            self.inner.output(slice);
            if let Some(event) = event
                && let Some(writer) = self.writer.upgrade()
            {
                let line = self.inner.cursor_line();
                writer.handle_osc133(event, line);
            }
            rest = rest.get(consumed..).unwrap_or_default();
            if consumed == 0 {
                break;
            }
        }
    }

    fn exited(&self, code: Option<u32>) {
        if let Some(writer) = self.writer.upgrade() {
            writer.on_exited();
        }
        self.inner.exited(code);
    }

    fn cursor_line(&self) -> Option<usize> {
        self.inner.cursor_line()
    }
}
