use std::time::Instant;

use super::status::{Failure, exchange};
use super::{Client, PipeError, QueryTerm, Request, Response};

/// Bumped by hand whenever the service's behaviour changes; Navo asks for an update when it is higher.
pub const SERVICE_REVISION: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServiceVersion {
    Known {
        revision: u32,
        build: String,
    },
    /// The service predates `VersionV1`: it dropped the request.
    Older,
}

/// The build id of this binary, or `dev`.
pub fn build_id() -> &'static str {
    option_env!("NAVO_BUILD").unwrap_or("dev")
}

impl Request {
    pub fn wants_version(&self) -> bool {
        matches!(self.terms.as_slice(), [QueryTerm::VersionV1])
    }
}

impl Client {
    /// Asks the running service for its revision. No pipe, or a timeout, is an error.
    pub fn version(deadline: Instant) -> Result<ServiceVersion, PipeError> {
        read_version(|request| exchange(request, 1, deadline))
    }
}

fn read_version(
    ask: impl FnOnce(Request) -> Result<Vec<Response>, Failure>,
) -> Result<ServiceVersion, PipeError> {
    let request = Request {
        terms: vec![QueryTerm::VersionV1],
    };
    match ask(request) {
        Ok(responses) => decode_version(responses),
        Err(Failure::Connect(error))
        | Err(Failure::Exchange(error @ (PipeError::TimedOut | PipeError::Cancelled))) => {
            Err(error)
        }
        Err(Failure::Exchange(_)) => Ok(ServiceVersion::Older),
    }
}

fn decode_version(responses: Vec<Response>) -> Result<ServiceVersion, PipeError> {
    match <[Response; 1]>::try_from(responses) {
        Ok(
            [
                Response::VersionV1 {
                    generation: 1,
                    revision,
                    build,
                },
            ],
        ) => Ok(ServiceVersion::Known { revision, build }),
        _ => Err(PipeError::InvalidResponse),
    }
}

#[cfg(test)]
#[path = "version_tests.rs"]
mod tests;
