use std::{
    sync::{PoisonError, mpsc},
    time::Instant,
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

#[derive(Default)]
struct Session {
    pending: std::sync::Mutex<Pending>,
    stopped: CancelToken,
}

pub(super) fn serve_session(
    pipe: &platform::Pipe,
    handler: &(impl Fn(&Request, &CancelToken) -> Reply + Sync),
    tokens: &[&CancelToken],
) -> Result<(), PipeError> {
    let session = Session::default();
    let (wake, events) = mpsc::sync_channel(1);
    std::thread::scope(|scope| {
        let _stop = StopOnDrop(session.stopped.clone());
        let reader = scope.spawn(|| read_queries(pipe, &session, tokens, wake));
        let result = send_replies(pipe, handler, &session, events);
        session.stopped.cancel();
        let read_result = reader.join().map_err(|_| PipeError::NotAvailable)?;
        result.and(connection_result(read_result))
    })
}

fn read_queries(
    pipe: &platform::Pipe,
    session: &Session,
    tokens: &[&CancelToken],
    wake: mpsc::SyncSender<()>,
) -> Result<(), PipeError> {
    let tokens: Vec<_> = tokens.iter().copied().chain([&session.stopped]).collect();
    let result = (|| {
        while !session.stopped.is_cancelled() {
            let query = pipe.read(None, &Cancellation::Tokens(&tokens))?;
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
    pipe: &platform::Pipe,
    handler: &(impl Fn(&Request, &CancelToken) -> Reply + Sync),
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
        let mut reply = answer_request(&query.request, handler, &cancel);
        if cancel.is_cancelled() || session.stopped.is_cancelled() {
            continue;
        }
        let hits = prepare_hits(query.generation, &mut reply)?;
        let deadline = Some(Instant::now() + QUERY_TIMEOUT);
        pipe.write(&hits, deadline)?;
        pipe.write(
            &Response::Status {
                generation: query.generation,
                status: reply.status,
            },
            deadline,
        )?;
    }
    Ok(())
}

pub(super) fn answer_request(
    request: &Request,
    handler: &(impl Fn(&Request, &CancelToken) -> Reply + Sync),
    cancel: &CancelToken,
) -> Reply {
    if request.within_limits() {
        return handler(request, cancel);
    }
    Reply {
        hits: Vec::new(),
        status: Status {
            volumes: vec![VolumeStatus {
                errors: vec!["index pipe request exceeds limits".into()],
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

pub(super) fn connection_result(result: Result<(), PipeError>) -> Result<(), PipeError> {
    match result {
        Err(PipeError::Cancelled | PipeError::NotAvailable | PipeError::Frame(_)) => Ok(()),
        other => other,
    }
}

pub(super) struct StopOnDrop(pub(super) CancelToken);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
