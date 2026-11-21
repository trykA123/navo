//! Fetch caps: a byte and time `Budget`, the `Capped` wrapper every `Http` backend runs inside,
//! and the `DeadlineGuard` timer that interrupts gix at the deadline (NC221).

use std::io::{self, BufRead, Read};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use gix_transport::client::blocking_io::http::{GetResponse, Http, PostBodyDataKind, PostResponse};

use crate::{HttpStatus, RemoteProblem};

/// The caps of one fetch: response bytes and wall-clock time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_bytes: u64,
    pub deadline: Duration,
}

impl Limits {
    /// The product caps: 256 MiB of response bytes and 300 s.
    pub const PRODUCT: Self = Self {
        max_bytes: 256 * 1024 * 1024,
        deadline: Duration::from_secs(300),
    };
}

/// The state of one fetch's caps, shared through an `Arc` by `Capped` and `DeadlineGuard`.
#[derive(Debug)]
pub struct Budget {
    limits: Limits,
    used: AtomicU64,
    start: Instant,
    problem: OnceLock<RemoteProblem>,
}

impl Budget {
    /// Starts a budget now with `limits`.
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            used: AtomicU64::new(0),
            start: Instant::now(),
            problem: OnceLock::new(),
        }
    }

    /// The cap that stopped the fetch, if any; the first one set wins.
    pub fn problem(&self) -> Option<RemoteProblem> {
        self.problem.get().copied()
    }

    /// Records `problem` (unless one is already set) and returns the read error for it.
    fn fail(&self, problem: RemoteProblem) -> io::Error {
        let _ = self.problem.set(problem);
        let (kind, text) = match self.problem() {
            Some(RemoteProblem::TimedOut) => (io::ErrorKind::TimedOut, "fetch deadline passed"),
            _ => (io::ErrorKind::FileTooLarge, "response over the byte cap"),
        };
        io::Error::new(kind, text)
    }

    /// The time left before the deadline (zero once it has passed).
    fn remaining(&self) -> Duration {
        self.limits.deadline.saturating_sub(self.start.elapsed())
    }

    /// Counts `bytes` more response bytes; fails when the total goes over `max_bytes`.
    pub(crate) fn charge(&self, bytes: usize) -> io::Result<()> {
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
        let total = self
            .used
            .fetch_add(bytes, Ordering::SeqCst)
            .saturating_add(bytes);
        if total > self.limits.max_bytes {
            return Err(self.fail(RemoteProblem::TooLarge));
        }
        Ok(())
    }
}

/// A response reader that charges every byte it hands out to a `Budget` and stops at its caps.
pub struct CappedRead<R> {
    inner: R,
    budget: Arc<Budget>,
    /// Bytes of the inner buffer already charged but not yet consumed.
    charged: usize,
}

impl<R: BufRead> BufRead for CappedRead<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if self.budget.remaining().is_zero() {
            return Err(self.budget.fail(RemoteProblem::TimedOut));
        }
        let buf = self.inner.fill_buf()?;
        if buf.len() > self.charged {
            self.budget.charge(buf.len() - self.charged)?;
            self.charged = buf.len();
        }
        Ok(buf)
    }

    fn consume(&mut self, amount: usize) {
        self.charged = self.charged.saturating_sub(amount);
        self.inner.consume(amount);
    }
}

impl<R: BufRead> Read for CappedRead<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        let buf = self.fill_buf()?;
        let n = buf.len().min(out.len());
        out[..n].copy_from_slice(&buf[..n]);
        self.consume(n);
        Ok(n)
    }
}

/// An `Http` backend wrapped so that its response headers and bodies obey one `Budget`.
pub struct Capped<H> {
    inner: H,
    budget: Arc<Budget>,
}

impl<H> Capped<H> {
    /// Wraps `inner`; every response it returns is charged to `budget`.
    pub fn new(inner: H, budget: Arc<Budget>) -> Self {
        Self { inner, budget }
    }

    /// Wraps one response reader in the budget.
    fn wrap<R>(&self, inner: R) -> CappedRead<R> {
        CappedRead {
            inner,
            budget: Arc::clone(&self.budget),
            charged: 0,
        }
    }
}

impl<H: Http> Http for Capped<H> {
    type Headers = CappedRead<H::Headers>;
    type ResponseBody = CappedRead<H::ResponseBody>;
    type PostBody = H::PostBody;

    fn get(
        &mut self,
        url: &str,
        base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> gix::Result<GetResponse<Self::Headers, Self::ResponseBody>> {
        self.before_call()?;
        let response = self.inner.get(url, base_url, headers)?;
        self.after_call()?;
        Ok(GetResponse {
            headers: self.wrap(response.headers),
            body: self.wrap(response.body),
        })
    }

    fn post(
        &mut self,
        url: &str,
        base_url: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
        body: PostBodyDataKind,
    ) -> gix::Result<PostResponse<Self::Headers, Self::ResponseBody, Self::PostBody>> {
        self.before_call()?;
        let response = self.inner.post(url, base_url, headers, body)?;
        self.after_call()?;
        Ok(PostResponse {
            post_body: response.post_body,
            headers: self.wrap(response.headers),
            body: self.wrap(response.body),
        })
    }

    fn configure(&mut self, config: &dyn std::any::Any) -> gix::Result {
        self.inner.configure(config)
    }

    /// Always `None`: a followed redirect already failed the call, so gix never changes host.
    fn redirected_base_url(&self) -> Option<String> {
        None
    }
}

impl<H: Http> Capped<H> {
    /// Fails with `TimedOut` once the deadline has passed, before the backend connects or sends.
    fn before_call(&self) -> gix::Result<()> {
        if self.budget.remaining().is_zero() {
            let err = self.budget.fail(RemoteProblem::TimedOut);
            return Err(gix::Error::from_error(err));
        }
        Ok(())
    }

    /// Fails as HTTP 302 (`Redirected`) when the backend followed a redirect to another base.
    fn after_call(&self) -> gix::Result<()> {
        if self.inner.redirected_base_url().is_some() {
            return Err(gix::Error::from_error(io::Error::other(HttpStatus(302))));
        }
        Ok(())
    }
}

/// A timer thread that records `TimedOut` and sets gix's interrupt flag at the budget's deadline.
/// Drop it before reading the budget's problem: dropping stops the thread and joins it.
pub struct DeadlineGuard {
    timer: Option<(mpsc::Sender<()>, JoinHandle<()>)>,
}

impl DeadlineGuard {
    /// Starts the timer for `budget`; at the deadline it sets `interrupt`.
    /// Fails when the system cannot start a thread.
    pub fn start(budget: Arc<Budget>, interrupt: Arc<AtomicBool>) -> io::Result<Self> {
        let (stop, stopped) = mpsc::channel::<()>();
        let thread = std::thread::Builder::new()
            .name("sidewise-fetch-deadline".into())
            .spawn(move || {
                // Only a timeout fires; a dropped sender (Disconnected) means the fetch ended first.
                if let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(budget.remaining()) {
                    let _ = budget.fail(RemoteProblem::TimedOut);
                    interrupt.store(true, Ordering::SeqCst);
                }
            })?;
        Ok(Self {
            timer: Some((stop, thread)),
        })
    }
}

impl Drop for DeadlineGuard {
    fn drop(&mut self) {
        if let Some((stop, thread)) = self.timer.take() {
            drop(stop);
            // The timer body cannot panic; a join error leaves nothing to clean up.
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests;
