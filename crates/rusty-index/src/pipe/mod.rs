mod cancel;
pub mod frames;
mod session;
#[cfg(not(windows))]
mod unix;
#[cfg(windows)]
mod windows;

use std::time::Instant;

pub use cancel::CancelToken;
use cancel::Cancellation;
use session::StopOnDrop;
#[cfg(test)]
use session::{Pending, answer_request, prepare_hits};
#[cfg(not(windows))]
use unix as platform;
#[cfg(windows)]
use windows as platform;

pub const PIPE_NAME: &str = r"\\.\pipe\ferrite-index";
pub const MAX_HITS: usize = 500;
pub const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(50);
pub const QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(200);

#[derive(Debug, thiserror::Error)]
pub enum PipeError {
    #[error("index service not available")]
    NotAvailable,
    #[error("index service timed out")]
    TimedOut,
    #[error("index pipe operation cancelled")]
    Cancelled,
    #[error("invalid installing user SID")]
    InvalidSid,
    #[error("unexpected index service response")]
    InvalidResponse,
    #[error(transparent)]
    Frame(#[from] frames::FrameError),
    #[error("index pipe I/O: {0}")]
    Io(#[from] std::io::Error),
}

pub use frames::{Hit, Query, QueryTerm, Reply, Request, Response, Status, VolumeStatus};

impl Request {
    fn within_limits(&self) -> bool {
        self.terms.len() <= 64
            && self.terms.iter().all(|term| match term {
                QueryTerm::Name(text) | QueryTerm::Path(text) => text.len() <= 1024,
                QueryTerm::Ext(extensions) => {
                    extensions.len() <= 1024
                        && extensions.iter().map(String::len).sum::<usize>()
                            + extensions.len().saturating_sub(1)
                            <= 1024
                }
            })
    }

    pub fn terms(&self) -> Vec<crate::search::Term<'_>> {
        self.terms
            .iter()
            .map(|term| match term {
                QueryTerm::Name(text) => crate::search::Term::Name(text),
                QueryTerm::Path(text) => crate::search::Term::Path(text),
                QueryTerm::Ext(extensions) => crate::search::Term::Ext(extensions),
            })
            .collect()
    }
}

pub struct Client {
    pipe: Option<platform::Pipe>,
    pending: Option<(u64, Instant)>,
}

impl Client {
    pub fn connect() -> Result<Self, PipeError> {
        Self::connect_at(PIPE_NAME)
    }

    pub(super) fn connect_at(name: &str) -> Result<Self, PipeError> {
        Ok(Self {
            pipe: Some(platform::Pipe::connect(name)?),
            pending: None,
        })
    }

    pub fn send_query(&mut self, query: &Query) -> Result<(), PipeError> {
        let deadline = Instant::now() + QUERY_TIMEOUT;
        let result = self
            .pipe
            .as_ref()
            .ok_or(PipeError::NotAvailable)?
            .write(query, Some(deadline));
        if result.is_err() {
            self.pipe.take();
            self.pending = None;
        } else {
            self.pending = Some((query.generation, deadline));
        }
        result
    }

    pub fn receive(&mut self, cancel: &impl Fn() -> bool) -> Result<Reply, PipeError> {
        self.pipe.as_ref().ok_or(PipeError::NotAvailable)?;
        let (generation, deadline) = self.pending.take().ok_or(PipeError::InvalidResponse)?;
        let result = self.read_reply(generation, deadline, cancel);
        if result
            .as_ref()
            .is_err_and(|error| !matches!(error, PipeError::Cancelled))
        {
            self.pipe.take();
        }
        result
    }

    fn read_reply(
        &self,
        generation: u64,
        deadline: Instant,
        cancel: &impl Fn() -> bool,
    ) -> Result<Reply, PipeError> {
        let pipe = self.pipe.as_ref().ok_or(PipeError::NotAvailable)?;
        let (mut hits, mut status) = (None, None);
        while hits.is_none() || status.is_none() {
            match pipe.read(Some(deadline), &Cancellation::Poll(cancel))? {
                Response::Hits {
                    generation: found,
                    hits: batch,
                } if found == generation && hits.is_none() && batch.len() <= MAX_HITS => {
                    hits = Some(batch)
                }
                Response::Status {
                    generation: found,
                    status: update,
                } if found == generation && status.is_none() => status = Some(update),
                Response::Hits {
                    generation: old, ..
                }
                | Response::Status {
                    generation: old, ..
                } if old < generation => {}
                _ => return Err(PipeError::InvalidResponse),
            }
        }
        Ok(Reply {
            hits: hits.unwrap_or_default(),
            status: status.unwrap_or_default(),
        })
    }

    pub fn query(&mut self, query: &Query) -> Result<Reply, PipeError> {
        self.send_query(query)?;
        self.receive(&|| false)
    }
}

pub struct Server {
    pipes: Vec<platform::Pipe>,
}

impl Server {
    pub fn bind(installing_user_sid: &str) -> Result<Self, PipeError> {
        Self::bind_at(PIPE_NAME, installing_user_sid)
    }

    pub(super) fn bind_at(name: &str, sid: &str) -> Result<Self, PipeError> {
        Ok(Self {
            pipes: platform::Pipe::bind_instances(name, sid)?,
        })
    }

    /// Serves connections until cancelled; malformed or truncated frames drop only their connection.
    pub fn serve(
        &mut self,
        handler: &(impl Fn(&Request, &CancelToken) -> Reply + Sync),
        cancel: &CancelToken,
    ) -> Result<(), PipeError> {
        let stopped = CancelToken::new();
        std::thread::scope(|scope| {
            let mut workers = Vec::new();
            for pipe in &self.pipes {
                let stopped = &stopped;
                workers.push(scope.spawn(move || serve_instance(pipe, handler, cancel, stopped)));
            }
            let mut result = Ok(());
            for worker in workers {
                let next = worker
                    .join()
                    .map_err(|_| PipeError::NotAvailable)
                    .and_then(|result| result);
                result = result.and(next);
            }
            result
        })
    }
}

fn serve_instance(
    pipe: &impl session::Transport,
    handler: &(impl Fn(&Request, &CancelToken) -> Reply + Sync),
    cancel: &CancelToken,
    stopped: &CancelToken,
) -> Result<(), PipeError> {
    let _stop = StopOnDrop(stopped.clone());
    let tokens = &[cancel, stopped];
    loop {
        match pipe.accept(&Cancellation::Tokens(tokens)) {
            Err(PipeError::Cancelled) => return Ok(()),
            Err(error) => return Err::<(), _>(error).and(pipe.disconnect()),
            Ok(()) => {}
        }
        let result = session::serve_session(pipe, handler, tokens);
        if let Err(error) = result.and(pipe.disconnect()) {
            tracing::debug!(%error, "index pipe client disconnected");
        }
    }
}

#[cfg(test)]
mod tests;
