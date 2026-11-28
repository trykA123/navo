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

    pub fn wants_status_v2(&self) -> bool {
        matches!(self.terms.as_slice(), [QueryTerm::StatusV2])
    }
}

pub(super) enum Failure {
    Connect(PipeError),
    Exchange(PipeError),
}

impl Client {
    /// Reads every volume's status with `total_files`; asks `StatusV2` first and falls back to
    /// `StatusV1` and then a plain query when talking to an older service.
    pub fn status(deadline: Instant) -> Result<Status, PipeError> {
        Self::status_v2(deadline).map(|(status, _)| status)
    }

    /// Reads every volume's status and optional `StatusV2` detail; older services fall back to
    /// `StatusV1` or a plain query with `None` detail.
    pub fn status_v2(
        deadline: Instant,
    ) -> Result<(Status, Option<Vec<super::VolumeDetail>>), PipeError> {
        read_status_v2(|request, replies| exchange(request, replies, deadline))
    }
}

pub(super) fn exchange(
    request: Request,
    replies: usize,
    deadline: Instant,
) -> Result<Vec<Response>, Failure> {
    let pipe = platform::Pipe::connect_before(PIPE_NAME, deadline).map_err(Failure::Connect)?;
    exchange_on(&pipe, request, replies, deadline).map_err(Failure::Exchange)
}

/// The responses of one exchange and the id of the process that served them.
pub(super) struct Identified {
    pub server: u32,
    pub responses: Result<Vec<Response>, PipeError>,
}

/// Like `exchange`, on one connection that also names its server process. An outer error means
/// no verified connection or no server id; the exchange itself is in `responses`.
pub(super) fn exchange_identified(
    request: Request,
    replies: usize,
    deadline: Instant,
) -> Result<Identified, PipeError> {
    let pipe = platform::Pipe::connect_before(PIPE_NAME, deadline)?;
    let server = pipe.server_process_id()?;
    let responses = exchange_on(&pipe, request, replies, deadline);
    Ok(Identified { server, responses })
}

fn exchange_on(
    pipe: &platform::Pipe,
    request: Request,
    replies: usize,
    deadline: Instant,
) -> Result<Vec<Response>, PipeError> {
    let query = Query {
        request,
        generation: 1,
    };
    pipe.write(&query, Some(deadline))?;
    (0..replies)
        .map(|_| pipe.read(Some(deadline), &Cancellation::Poll(&|| false)))
        .collect()
}

pub(super) fn read_status_v2(
    mut ask: impl FnMut(Request, usize) -> Result<Vec<Response>, Failure>,
) -> Result<(Status, Option<Vec<super::VolumeDetail>>), PipeError> {
    let v2 = Request {
        terms: vec![QueryTerm::StatusV2],
    };
    match ask(v2, 1) {
        Ok(responses) => decode_v2(responses).map(|(status, detail)| (status, Some(detail))),
        Err(Failure::Connect(error))
        | Err(Failure::Exchange(error @ (PipeError::TimedOut | PipeError::Cancelled))) => {
            Err(error)
        }
        Err(Failure::Exchange(_)) => read_status_v1(ask).map(|status| (status, None)),
    }
}

#[cfg(test)]
fn read_status(
    ask: impl FnMut(Request, usize) -> Result<Vec<Response>, Failure>,
) -> Result<Status, PipeError> {
    read_status_v2(ask).map(|(status, _)| status)
}

fn read_status_v1(
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

fn decode_v2(responses: Vec<Response>) -> Result<(Status, Vec<super::VolumeDetail>), PipeError> {
    match <[Response; 1]>::try_from(responses) {
        Ok(
            [
                Response::StatusV2 {
                    generation: 1,
                    status,
                    detail,
                },
            ],
        ) => Ok((status, detail)),
        _ => Err(PipeError::InvalidResponse),
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
