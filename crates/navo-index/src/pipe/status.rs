use std::time::Instant;

use super::{
    Cancellation, Client, PIPE_NAME, PipeError, Query, QueryTerm, Request, Response, Status,
    VolumeStatus, platform,
};

impl VolumeStatus {
    /// Raises `total_files` to `entries`: the MFT count is an estimate and a percent never passes 100.
    pub fn clamp_total(&mut self) {
        if let Some(total) = &mut self.total_files {
            *total = (*total).max(self.entries);
        }
    }
}

impl Request {
    pub fn wants_status(&self) -> bool {
        matches!(self.terms.as_slice(), [QueryTerm::StatusV1])
    }
}

pub(super) enum Failure {
    Connect(PipeError),
    Exchange(PipeError),
}

impl Client {
    /// Reads every volume's status with `total_files`; a service older than `StatusV1` drops
    /// that request, so the status is read again through a plain query, without totals.
    pub fn status(deadline: Instant) -> Result<Status, PipeError> {
        read_status(|request, replies| exchange(request, replies, deadline))
    }
}

pub(super) fn exchange(
    request: Request,
    replies: usize,
    deadline: Instant,
) -> Result<Vec<Response>, Failure> {
    let pipe = platform::Pipe::connect_before(PIPE_NAME, deadline).map_err(Failure::Connect)?;
    let query = Query {
        request,
        generation: 1,
    };
    pipe.write(&query, Some(deadline))
        .map_err(Failure::Exchange)?;
    (0..replies)
        .map(|_| pipe.read(Some(deadline), &Cancellation::Poll(&|| false)))
        .collect::<Result<_, _>>()
        .map_err(Failure::Exchange)
}

fn read_status(
    mut ask: impl FnMut(Request, usize) -> Result<Vec<Response>, Failure>,
) -> Result<Status, PipeError> {
    let detailed = Request {
        terms: vec![QueryTerm::StatusV1],
    };
    match ask(detailed, 1) {
        Ok(responses) => decode_detailed(responses),
        Err(Failure::Connect(error))
        | Err(Failure::Exchange(error @ (PipeError::TimedOut | PipeError::Cancelled))) => {
            Err(error)
        }
        Err(Failure::Exchange(_)) => match ask(Request::default(), 2) {
            Ok(responses) => decode_plain(responses),
            Err(Failure::Connect(error) | Failure::Exchange(error)) => Err(error),
        },
    }
}

fn decode_detailed(responses: Vec<Response>) -> Result<Status, PipeError> {
    match <[Response; 1]>::try_from(responses) {
        Ok(
            [
                Response::StatusV1 {
                    generation: 1,
                    status,
                },
            ],
        ) => Ok(status),
        _ => Err(PipeError::InvalidResponse),
    }
}

fn decode_plain(responses: Vec<Response>) -> Result<Status, PipeError> {
    match <[Response; 2]>::try_from(responses) {
        Ok(
            [
                Response::Hits { generation: 1, .. },
                Response::Status {
                    generation: 1,
                    status,
                },
            ],
        ) => Ok(status),
        _ => Err(PipeError::InvalidResponse),
    }
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
