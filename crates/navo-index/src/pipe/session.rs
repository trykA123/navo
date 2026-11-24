use std::{
    sync::{
        Arc, PoisonError,
        atomic::{AtomicUsize, Ordering::Relaxed},
        mpsc,
    },
    time::{Duration, Instant},
};

use super::{
    CancelToken, Cancellation, MAX_HITS, PipeError, QUERY_TIMEOUT, Query, Reply, Request, Response,
    SERVICE_REVISION, Status, VolumeStatus, build_id, frames, platform,
};
use crate::latest::Latest;

#[derive(Default)]
pub(super) struct Pending {
    pub(super) generation: Option<u64>,
    pub(super) query: Option<Query>,
    pub(super) cancel: CancelToken,
}

impl Pending {
    pub(super) fn accept(&mut self, query: Query) {
        if self.generation.is_some_and(|old| query.generation <= old) {
            return;
        }
        self.cancel.cancel();
        self.cancel = CancelToken::new();
        self.generation = Some(query.generation);
        self.query = Some(query);
    }
}

const REPLY_POLL: Duration = Duration::from_millis(5);

pub(super) struct Job {
    request: Request,
    cancel: CancelToken,
    reply: mpsc::SyncSender<Reply>,
}

pub(super) fn answer_jobs(
    jobs: Arc<Latest<Job>>,
    handler: &(impl Fn(&Request, &CancelToken) -> Reply + Sync),
) {
    while let Some(job) = jobs.next() {
        if !job.cancel.is_cancelled() {
            let answered = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                answer_request(&job.request, handler, &job.cancel)
            }));
            let reply = answered.unwrap_or_else(|_| {
                tracing::error!("index pipe answer handler panicked");
                error_reply("index search failed")
            });
            let _ = job.reply.send(reply);
        }
    }
}

#[derive(Default)]
struct Session {
    pending: std::sync::Mutex<Pending>,
    stopped: CancelToken,
}

static OPEN_SESSIONS: AtomicUsize = AtomicUsize::new(0);

struct Open;

impl Open {
    fn count() -> Self {
        OPEN_SESSIONS.fetch_add(1, Relaxed);
        Self
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        OPEN_SESSIONS.fetch_sub(1, Relaxed);
    }
}

pub(super) fn memory() -> (usize, usize) {
    let open = OPEN_SESSIONS.load(Relaxed);
    (open * size_of::<Session>(), open)
}

/// How long a connected client may stay silent before the server drops it and accepts the next.
pub(super) const IDLE: Duration = Duration::from_secs(30);

/// One server pipe instance: accepts a client, reads its queries and writes the answers.
pub(super) trait Transport: Sync {
    fn accept(&self, cancel: &Cancellation<'_>) -> Result<(), PipeError>;
    fn disconnect(&self) -> Result<(), PipeError>;
    /// Reads the next query; fails with `TimedOut` once `deadline` passes with no whole frame.
    fn read_query(
        &self,
        deadline: Option<Instant>,
        cancel: &Cancellation<'_>,
    ) -> Result<Query, PipeError>;
    fn write_response(&self, response: &Response, deadline: Instant) -> Result<(), PipeError>;

    /// The silence limit for one client; tests shorten it instead of waiting 30 s.
    fn idle_limit(&self) -> Duration {
        IDLE
    }
}

impl Transport for platform::Pipe {
    fn accept(&self, cancel: &Cancellation<'_>) -> Result<(), PipeError> {
        self.accept(cancel)
    }

    fn disconnect(&self) -> Result<(), PipeError> {
        self.disconnect()
    }

    fn read_query(
        &self,
        deadline: Option<Instant>,
        cancel: &Cancellation<'_>,
    ) -> Result<Query, PipeError> {
        self.read(deadline, cancel)
    }

    fn write_response(&self, response: &Response, deadline: Instant) -> Result<(), PipeError> {
        self.write(response, Some(deadline))
    }
}

pub(super) fn serve_session(
    pipe: &impl Transport,
    answers: &Latest<Job>,
    tokens: &[&CancelToken],
) -> Result<(), PipeError> {
    let _open = Open::count();
    let session = Session::default();
    let (wake, events) = mpsc::sync_channel(1);
    std::thread::scope(|scope| {
        let _stop = StopOnDrop(session.stopped.clone());
        let reader = scope.spawn(|| read_queries(pipe, &session, tokens, wake));
        let result = send_replies(pipe, answers, &session, &events);
        session.stopped.cancel();
        let read_result = reader.join().map_err(|_| PipeError::NotAvailable)?;
        result.and(read_result)
    })
}

fn read_queries(
    pipe: &impl Transport,
    session: &Session,
    tokens: &[&CancelToken],
    wake: mpsc::SyncSender<()>,
) -> Result<(), PipeError> {
    let tokens: Vec<_> = tokens.iter().copied().chain([&session.stopped]).collect();
    let result = (|| {
        while !session.stopped.is_cancelled() {
            let idle_until = Instant::now() + pipe.idle_limit();
            let query = pipe.read_query(Some(idle_until), &Cancellation::Tokens(&tokens))?;
            session
                .pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .accept(query);
            let _ = wake.try_send(());
        }
        Ok(())
    })();
    session
        .pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .cancel
        .cancel();
    session.stopped.cancel();
    drop(wake);
    result
}

fn send_replies(
    pipe: &impl Transport,
    answers: &Latest<Job>,
    session: &Session,
    events: &mpsc::Receiver<()>,
) -> Result<(), PipeError> {
    while !session.stopped.is_cancelled() {
        let Some((query, cancel)) = take_next(session) else {
            if events.recv().is_err() {
                break;
            }
            continue;
        };
        if cancel.is_cancelled() || session.stopped.is_cancelled() {
            continue;
        }
        if query.request.wants_version() {
            let version = Response::VersionV1 {
                generation: query.generation,
                revision: SERVICE_REVISION,
                build: build_id().into(),
            };
            pipe.write_response(&version, Instant::now() + QUERY_TIMEOUT)?;
            continue;
        }
        let detailed_status = query.request.wants_status();
        let (reply, answered) = mpsc::sync_channel(1);
        if !answers.publish(Job {
            request: query.request,
            cancel: cancel.clone(),
            reply,
        }) {
            return Err(PipeError::NotAvailable);
        }
        let Some(reply) = wait_for_reply(&answered, &cancel, session) else {
            continue;
        };
        if detailed_status {
            let status = Response::StatusV1 {
                generation: query.generation,
                status: reply.status,
            };
            pipe.write_response(&status, Instant::now() + QUERY_TIMEOUT)?;
        } else {
            write_reply(pipe, query.generation, reply)?;
        }
    }
    Ok(())
}

fn take_next(session: &Session) -> Option<(Query, CancelToken)> {
    let mut state = session
        .pending
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    state
        .query
        .take()
        .map(|query| (query, state.cancel.clone()))
}

fn write_reply(pipe: &impl Transport, generation: u64, mut reply: Reply) -> Result<(), PipeError> {
    let deadline = Instant::now() + QUERY_TIMEOUT;
    if let Some(totals) = reply.totals {
        pipe.write_response(
            &Response::FolderTotalsV1 {
                generation,
                reply: totals,
            },
            deadline,
        )?;
        return Ok(());
    }
    if let Some(report) = reply.memory {
        return pipe.write_response(&Response::MemoryV1 { generation, report }, deadline);
    }
    let hits = prepare_hits(generation, &mut reply)?;
    let deadline = Instant::now() + QUERY_TIMEOUT;
    pipe.write_response(&hits, deadline)?;
    pipe.write_response(
        &Response::Status {
            generation,
            status: reply.status,
        },
        deadline,
    )?;
    Ok(())
}

fn wait_for_reply(
    answered: &mpsc::Receiver<Reply>,
    cancel: &CancelToken,
    session: &Session,
) -> Option<Reply> {
    loop {
        match answered.recv_timeout(REPLY_POLL) {
            Ok(reply) if !cancel.is_cancelled() && !session.stopped.is_cancelled() => {
                return Some(reply);
            }
            Err(mpsc::RecvTimeoutError::Timeout)
                if !cancel.is_cancelled() && !session.stopped.is_cancelled() => {}
            _ => return None,
        }
    }
}

pub(super) fn answer_request(
    request: &Request,
    handler: &(impl Fn(&Request, &CancelToken) -> Reply + Sync),
    cancel: &CancelToken,
) -> Reply {
    if request.within_limits() {
        return handler(request, cancel);
    }
    error_reply("index pipe request exceeds limits")
}

fn error_reply(message: &str) -> Reply {
    Reply {
        status: Status {
            volumes: vec![VolumeStatus {
                errors: vec![message.into()],
                ..VolumeStatus::default()
            }],
        },
        ..Reply::default()
    }
}

pub(super) fn prepare_hits(generation: u64, reply: &mut Reply) -> Result<Response, PipeError> {
    reply.hits.sort_by_key(|hit| std::cmp::Reverse(hit.score));
    reply.hits.truncate(MAX_HITS);
    let mut response = Response::Hits {
        generation,
        hits: std::mem::take(&mut reply.hits),
    };
    while postcard::experimental::serialized_size(&response).map_err(frames::FrameError::from)?
        > frames::MAX_FRAME_BYTES
    {
        let Response::Hits { hits, .. } = &mut response else {
            return Err(PipeError::InvalidResponse);
        };
        hits.pop();
    }
    Ok(response)
}

pub(super) struct StopOnDrop(pub(super) CancelToken);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    use super::*;

    const WAIT: Duration = Duration::from_secs(5);

    fn generation(request: &Request) -> u64 {
        let [super::super::QueryTerm::Name(name), ..] = request.terms.as_slice() else {
            panic!("test requests carry one name term");
        };
        name.parse().unwrap()
    }

    struct Stall {
        entered: mpsc::Sender<u64>,
        release: Mutex<mpsc::Receiver<()>>,
    }

    impl Stall {
        fn park_first(&self, first: &AtomicBool, generation: u64) {
            if first.swap(false, Ordering::SeqCst) {
                self.entered.send(generation).unwrap();
                self.release.lock().unwrap().recv().unwrap();
            }
        }
    }

    /// Hands out generations `0..1_000`, holding `1..` until the test opens the
    /// burst, then idles until the session stops.
    struct BurstPipe {
        next: Mutex<u64>,
        burst: Mutex<Option<mpsc::Receiver<()>>>,
        all_sent: mpsc::Sender<()>,
        written: mpsc::Sender<u64>,
    }

    impl BurstPipe {
        fn take_generation(&self) -> Option<u64> {
            let mut next = self.next.lock().unwrap();
            if *next >= 1_000 {
                return None;
            }
            let generation = *next;
            *next += 1;
            Some(generation)
        }
    }

    fn idle(cancel: &Cancellation<'_>) {
        while !cancel.is_cancelled() {
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    impl Transport for BurstPipe {
        fn accept(&self, _: &Cancellation<'_>) -> Result<(), PipeError> {
            Ok(())
        }

        fn disconnect(&self) -> Result<(), PipeError> {
            Ok(())
        }

        fn read_query(
            &self,
            _: Option<Instant>,
            cancel: &Cancellation<'_>,
        ) -> Result<Query, PipeError> {
            let Some(generation) = self.take_generation() else {
                idle(cancel);
                return Err(PipeError::Cancelled);
            };
            if generation == 1
                && let Some(opened) = self.burst.lock().unwrap().take()
            {
                opened.recv().unwrap();
            }
            if generation == 999 {
                self.all_sent.send(()).unwrap();
            }
            Ok(Query {
                request: Request {
                    terms: vec![super::super::QueryTerm::Name(generation.to_string())],
                },
                generation,
            })
        }

        fn write_response(&self, response: &Response, _: Instant) -> Result<(), PipeError> {
            let generation = match response {
                Response::Hits { generation, .. }
                | Response::Status { generation, .. }
                | Response::FolderTotalsV1 { generation, .. }
                | Response::MemoryV1 { generation, .. }
                | Response::StatusV1 { generation, .. }
                | Response::VersionV1 { generation, .. } => *generation,
            };
            self.written
                .send(generation)
                .map_err(|_| PipeError::NotAvailable)
        }
    }

    #[test]
    fn a_thousand_generation_bumps_keep_one_pending_answer() {
        let (stalled_sender, stalled) = mpsc::channel();
        let (release_sender, releases) = mpsc::channel();
        let (burst_sender, burst) = mpsc::channel();
        let (all_sent, sent) = mpsc::channel();
        let (written, responses) = mpsc::channel();
        let stall = Arc::new(Stall {
            entered: stalled_sender,
            release: Mutex::new(releases),
        });
        let first = Arc::new(AtomicBool::new(true));
        let answered = Arc::new(Mutex::new(Vec::new()));
        let handler = {
            let stall = Arc::clone(&stall);
            let first = Arc::clone(&first);
            let answered = Arc::clone(&answered);
            move |request: &Request, _: &CancelToken| {
                let generation = generation(request);
                answered.lock().unwrap().push(generation);
                stall.park_first(&first, generation);
                Reply::default()
            }
        };
        let pipe = BurstPipe {
            next: Mutex::new(0),
            burst: Mutex::new(Some(burst)),
            all_sent,
            written,
        };
        let answers = Arc::new(Latest::default());
        let stop = CancelToken::new();
        std::thread::scope(|scope| {
            let worker = {
                let answers = Arc::clone(&answers);
                scope.spawn(move || answer_jobs(answers, &handler))
            };
            let session = {
                let answers = Arc::clone(&answers);
                let stop = stop.clone();
                scope.spawn(move || serve_session(&pipe, &answers, &[&stop]))
            };
            assert_eq!(stalled.recv_timeout(WAIT).unwrap(), 0);
            assert_eq!(answers.revision(), 1, "one job while the worker is stalled");
            burst_sender.send(()).unwrap();
            sent.recv_timeout(WAIT).unwrap();
            let deadline = Instant::now() + WAIT;
            while answers.revision() < 2 {
                assert!(
                    Instant::now() < deadline,
                    "the newest generation was never published"
                );
                std::thread::yield_now();
            }
            assert!(answers.pending(), "the newest job is the only one pending");
            answers.close();
            release_sender.send(()).unwrap();
            assert_eq!(responses.recv_timeout(WAIT).unwrap(), 999);
            assert_eq!(responses.recv_timeout(WAIT).unwrap(), 999);
            stop.cancel();
            assert!(worker.join().is_ok());
            assert!(session.join().is_ok());
        });
        assert_eq!(*answered.lock().unwrap(), [0, 999]);
    }
}
