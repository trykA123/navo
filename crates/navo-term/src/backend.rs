//! The seams of a session: `Backend` is the process side (a real PTY or a fake), `Sink`
//! receives what the process prints. `FakeBackend` lets UI tests run without a shell.

use std::sync::Arc;

use crate::{Size, TermError};

/// Non-blocking callback sending terminal query replies (DA1, DSR, OSC 10/11) back to the PTY.
pub type ReplyFn = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// The process side of a session. Dropping it ends the process.
pub trait Backend: Send {
    /// Sends bytes to the process as if typed.
    fn write(&mut self, bytes: &[u8]) -> Result<(), TermError>;
    /// Tells the process the visible size changed; callers debounce.
    fn resize(&mut self, size: Size) -> Result<(), TermError>;
}

/// Receives a backend's output on its sink thread. Calls arrive in order; `exited` comes
/// last, once, after all output.
pub trait Sink: Send + Sync {
    /// Bytes the process printed, in arbitrary chunks.
    fn output(&self, bytes: &[u8]);
    /// The process ended; `code` is `None` when the exit status could not be read.
    fn exited(&self, code: Option<u32>);
    /// Monotonic grid line (`history_size + cursor_row`) right after the latest `output` call,
    /// used to anchor OSC 133 marks for command blocks; `None` when the sink has no grid.
    fn cursor_line(&self) -> Option<usize> {
        None
    }
    /// Attaches a non-blocking PTY reply sender for terminal query responses (DA1, DSR, OSC 10/11).
    /// Replies bypass user input tracking so protocol handshakes never mark the prompt as typed.
    fn attach_reply(&self, _reply: ReplyFn) {}
}

#[cfg(any(test, feature = "test-support"))]
pub use fake::{FakeBackend, FakeHandle};

#[cfg(any(test, feature = "test-support"))]
mod fake {
    use std::sync::{Arc, Mutex, PoisonError};

    use super::{Backend, Sink};
    use crate::{Size, TermError};

    #[derive(Default)]
    struct Log {
        written: Vec<u8>,
        sizes: Vec<Size>,
        dropped: bool,
    }

    /// A backend that records writes and sizes and prints only what a test tells it to.
    pub struct FakeBackend {
        log: Arc<Mutex<Log>>,
    }

    /// The test's side of a `FakeBackend`: read what was sent, make the "process" print or exit.
    #[derive(Clone)]
    pub struct FakeHandle {
        log: Arc<Mutex<Log>>,
        sink: Arc<dyn Sink>,
    }

    impl FakeBackend {
        /// A fake whose output goes to `sink`.
        pub fn new(sink: Arc<dyn Sink>) -> (Self, FakeHandle) {
            let log = Arc::new(Mutex::new(Log::default()));
            let handle = FakeHandle {
                log: Arc::clone(&log),
                sink,
            };
            (Self { log }, handle)
        }

        fn with_log<T>(&self, f: impl FnOnce(&mut Log) -> T) -> T {
            f(&mut self.log.lock().unwrap_or_else(PoisonError::into_inner))
        }
    }

    impl Backend for FakeBackend {
        fn write(&mut self, bytes: &[u8]) -> Result<(), TermError> {
            self.with_log(|log| log.written.extend_from_slice(bytes));
            Ok(())
        }

        fn resize(&mut self, size: Size) -> Result<(), TermError> {
            size.check()?;
            self.with_log(|log| log.sizes.push(size));
            Ok(())
        }
    }

    impl Drop for FakeBackend {
        fn drop(&mut self) {
            self.with_log(|log| log.dropped = true);
        }
    }

    impl FakeHandle {
        /// Everything written to the backend so far.
        pub fn written(&self) -> Vec<u8> {
            self.with_log(|log| log.written.clone())
        }

        /// Every size the backend was resized to, in order.
        pub fn sizes(&self) -> Vec<Size> {
            self.with_log(|log| log.sizes.clone())
        }

        /// Whether the session dropped (and so ended) the backend.
        pub fn is_dropped(&self) -> bool {
            self.with_log(|log| log.dropped)
        }

        /// Makes the fake process print `bytes`.
        pub fn emit(&self, bytes: &[u8]) {
            self.sink.output(bytes);
        }

        /// Makes the fake process exit with `code`.
        pub fn exit(&self, code: u32) {
            self.sink.exited(Some(code));
        }

        /// Redirects future fake output and exit events through `sink` (for example a `PromptSink`).
        #[must_use]
        pub fn with_sink(mut self, sink: Arc<dyn Sink>) -> Self {
            self.sink = sink;
            self
        }

        fn with_log<T>(&self, f: impl FnOnce(&mut Log) -> T) -> T {
            f(&mut self.log.lock().unwrap_or_else(PoisonError::into_inner))
        }
    }
}
