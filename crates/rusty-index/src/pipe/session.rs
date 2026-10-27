use std::{
    sync::{PoisonError, mpsc},
    time::{Duration, Instant},
};

use super::{
    CancelToken, Cancellation, MAX_HITS, PipeError, QUERY_TIMEOUT, Query, Reply, Request, Response,
    Status, VolumeStatus, frames, platform,
};

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
    jobs: mpsc::Receiver<Job>,
    handler: &(impl Fn(&Request, &CancelToken) -> Reply + Sync),
) {
    for job in jobs {
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

pub(super) trait Transport: Sync {
    fn accept(&self, cancel: &Cancellation<'_>) -> Result<(), PipeError>;
    fn disconnect(&self) -> Result<(), PipeError>;
    fn read_query(&self, cancel: &Cancellation<'_>) -> Result<Query, PipeError>;
    fn write_response(&self, response: &Response, deadline: Instant) -> Result<(), PipeError>;
}

impl Transport for platform::Pipe {
    fn accept(&self, cancel: &Cancellation<'_>) -> Result<(), PipeError> {
        self.accept(cancel)
    }

    fn disconnect(&self) -> Result<(), PipeError> {
        self.disconnect()
    }

    fn read_query(&self, cancel: &Cancellation<'_>) -> Result<Query, PipeError> {
        self.read(None, cancel)
    }

    fn write_response(&self, response: &Response, deadline: Instant) -> Result<(), PipeError> {
        self.write(response, Some(deadline))
    }
}

pub(super) fn serve_session(
    pipe: &impl Transport,
    answers: &mpsc::Sender<Job>,
    tokens: &[&CancelToken],
) -> Result<(), PipeError> {
    let session = Session::default();
    let (wake, events) = mpsc::sync_channel(1);
    std::thread::scope(|scope| {
        let _stop = StopOnDrop(session.stopped.clone());
        let reader = scope.spawn(|| read_queries(pipe, &session, tokens, wake));
        let result = send_replies(pipe, answers, &session, events);
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
            let query = pipe.read_query(&Cancellation::Tokens(&tokens))?;
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
    result
}

fn send_replies(
    pipe: &impl Transport,
    answers: &mpsc::Sender<Job>,
    session: &Session,
    events: mpsc::Receiver<()>,
) -> Result<(), PipeError> {
    while !session.stopped.is_cancelled() {
        let next = {
            let mut state = session
                .pending
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            state
                .query
                .take()
                .map(|query| (query, state.cancel.clone()))
        };
        let Some((query, cancel)) = next else {
            if events.recv().is_err() {
                break;
            }
            continue;
        };
        if cancel.is_cancelled() || session.stopped.is_cancelled() {
            continue;
        }
        let (reply, answered) = mpsc::sync_channel(1);
        answers
            .send(Job {
                request: query.request,
                cancel: cancel.clone(),
                reply,
            })
            .map_err(|_| PipeError::NotAvailable)?;
        let Some(reply) = wait_for_reply(&answered, &cancel, session) else {
            continue;
        };
        write_reply(pipe, query.generation, reply)?;
    }
    Ok(())
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
        totals: None,
        hits: Vec::new(),
        status: Status {
            volumes: vec![VolumeStatus {
                errors: vec![message.into()],
                ..VolumeStatus::default()
            }],
        },
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
