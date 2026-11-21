//! Tests for the capped transport, the byte budget and the deadline guard.

#![allow(
    clippy::unwrap_used,
    reason = "test: a failed step should abort the test"
)]

use super::*;
use std::io::{BufReader, Cursor, Sink};

/// An in-memory `Http`: every response is `headers` then `body`, read 4 bytes at a time.
/// It counts its calls and can claim it followed a redirect to `redirect`.
struct Fake {
    headers: Vec<u8>,
    body: Vec<u8>,
    calls: usize,
    redirect: Option<String>,
}

type Reader = BufReader<Cursor<Vec<u8>>>;

impl Fake {
    fn reader(bytes: &[u8]) -> Reader {
        BufReader::with_capacity(4, Cursor::new(bytes.to_vec()))
    }
}

impl Http for Fake {
    type Headers = Reader;
    type ResponseBody = Reader;
    type PostBody = Sink;

    fn get(
        &mut self,
        _: &str,
        _: &str,
        _: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> gix::Result<GetResponse<Reader, Reader>> {
        self.calls += 1;
        Ok(GetResponse {
            headers: Self::reader(&self.headers),
            body: Self::reader(&self.body),
        })
    }

    fn post(
        &mut self,
        url: &str,
        base: &str,
        headers: impl IntoIterator<Item = impl AsRef<str>>,
        _: PostBodyDataKind,
    ) -> gix::Result<PostResponse<Reader, Reader, Sink>> {
        let got = self.get(url, base, headers)?;
        Ok(PostResponse {
            post_body: io::sink(),
            headers: got.headers,
            body: got.body,
        })
    }

    fn configure(&mut self, _: &dyn std::any::Any) -> gix::Result {
        Ok(())
    }

    fn redirected_base_url(&self) -> Option<String> {
        self.redirect.clone()
    }
}

fn capped(headers: usize, body: usize, limits: Limits) -> (Capped<Fake>, Arc<Budget>) {
    let budget = Arc::new(Budget::new(limits));
    let fake = Fake {
        headers: vec![b'h'; headers],
        body: vec![b'b'; body],
        calls: 0,
        redirect: None,
    };
    (Capped::new(fake, Arc::clone(&budget)), budget)
}

fn limits(max_bytes: u64, deadline_ms: u64) -> Limits {
    Limits {
        max_bytes,
        deadline: Duration::from_millis(deadline_ms),
    }
}

#[test]
fn product_limits() {
    assert_eq!(Limits::PRODUCT.max_bytes, 268_435_456);
    assert_eq!(Limits::PRODUCT.deadline, Duration::from_secs(300));
}

#[test]
fn body_within_cap_reads_whole() {
    let (mut http, budget) = capped(10, 30, limits(40, 60_000));
    let mut got = http.get("u", "b", ["x"]).unwrap();
    let mut out = Vec::new();
    got.headers.read_to_end(&mut out).unwrap();
    got.body.read_to_end(&mut out).unwrap();
    assert_eq!(out.len(), 40);
    assert_eq!(budget.problem(), None);
}

#[test]
fn body_over_cap_fails_too_large() {
    let (mut http, budget) = capped(10, 31, limits(40, 60_000));
    let mut got = http
        .post("u", "b", ["x"], PostBodyDataKind::Unbounded)
        .unwrap();
    let mut out = Vec::new();
    got.headers.read_to_end(&mut out).unwrap();
    let err = got.body.read_to_end(&mut out).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::FileTooLarge);
    assert!(
        out.len() <= 40,
        "handed out {} bytes over a 40-byte cap",
        out.len()
    );
    assert_eq!(budget.problem(), Some(RemoteProblem::TooLarge));
}

#[test]
fn read_after_deadline_fails_timed_out() {
    let (mut http, budget) = capped(0, 64, limits(1 << 20, 40));
    let mut got = http.get("u", "b", ["x"]).unwrap();
    let mut chunk = [0; 4];
    assert_eq!(got.body.read(&mut chunk).unwrap(), 4);
    std::thread::sleep(Duration::from_millis(60));
    let err = got.body.read(&mut chunk).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::TimedOut);
    assert_eq!(budget.problem(), Some(RemoteProblem::TimedOut));
}

#[test]
fn first_problem_wins() {
    let (mut http, budget) = capped(0, 64, limits(4, 0));
    assert!(http.get("u", "b", ["x"]).is_err());
    assert_eq!(budget.problem(), Some(RemoteProblem::TimedOut));
    let _ = budget.charge(100);
    assert_eq!(budget.problem(), Some(RemoteProblem::TimedOut));
}

#[test]
fn guard_sets_interrupt_at_deadline() {
    let budget = Arc::new(Budget::new(limits(1, 50)));
    let flag = Arc::new(AtomicBool::new(false));
    let guard = DeadlineGuard::start(Arc::clone(&budget), Arc::clone(&flag)).unwrap();
    assert!(!flag.load(Ordering::SeqCst));
    let start = Instant::now();
    while !flag.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(flag.load(Ordering::SeqCst), "interrupt not set after 5 s");
    assert_eq!(budget.problem(), Some(RemoteProblem::TimedOut));
    drop(guard);
}

#[test]
fn dropping_guard_ends_thread_without_interrupt() {
    let budget = Arc::new(Budget::new(limits(1, 3_600_000)));
    let flag = Arc::new(AtomicBool::new(false));
    let guard = DeadlineGuard::start(Arc::clone(&budget), Arc::clone(&flag)).unwrap();
    let start = Instant::now();
    drop(guard);
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "drop waited for the deadline"
    );
    assert_eq!(
        Arc::strong_count(&budget),
        1,
        "timer thread still holds the budget"
    );
    assert!(!flag.load(Ordering::SeqCst));
    assert_eq!(budget.problem(), None);
}

#[test]
fn call_after_deadline_never_reaches_backend() {
    let (mut http, budget) = capped(0, 64, limits(1 << 20, 0));
    assert!(http.get("u", "b", ["x"]).is_err());
    let post = http.post("u", "b", ["x"], PostBodyDataKind::Unbounded);
    assert!(post.is_err());
    assert_eq!(http.inner.calls, 0, "the backend ran after the deadline");
    assert_eq!(budget.problem(), Some(RemoteProblem::TimedOut));
}

#[test]
fn followed_redirect_fails_as_redirected() {
    let (mut http, budget) = capped(0, 64, limits(1 << 20, 60_000));
    http.inner.redirect = Some("https://elsewhere.invalid/repo.git".into());
    assert_eq!(
        http.redirected_base_url(),
        None,
        "a redirect passed through"
    );
    let err = http.get("u", "b", ["x"]).err().unwrap();
    assert_eq!(crate::problem_of(&err, &budget), RemoteProblem::Redirected);
    let post = http.post("u", "b", ["x"], PostBodyDataKind::Unbounded);
    let err = post.err().unwrap();
    assert_eq!(crate::problem_of(&err, &budget), RemoteProblem::Redirected);
}
