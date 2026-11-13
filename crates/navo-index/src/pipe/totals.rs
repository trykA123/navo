//! Queries folder aggregates and validates positional replies from the index service.

use std::time::Instant;

use super::{
    Cancellation, Client, FolderTotals, FolderTotalsReply, PIPE_NAME, PipeError, Query, QueryTerm,
    Request, Response, platform,
};

pub fn folder_paths_within_limits(paths: &[String]) -> bool {
    let mut components = 0;
    paths.len() <= 64
        && paths.iter().all(|path| {
            if path.len() > 32768 || path.contains('\0') {
                return false;
            }
            components += path.split(['/', '\\']).take(4097).count();
            components <= 4096
        })
}

impl Client {
    /// Queries positional totals before the deadline, rejecting Busy and invalid replies.
    pub fn folder_totals(
        paths: &[String],
        deadline: Instant,
    ) -> Result<Vec<Option<FolderTotals>>, PipeError> {
        decode_totals(query_totals(paths, deadline)?, paths.len())
    }

    /// Queries positional totals and rebuild Busy before the deadline; rejects invalid replies.
    pub fn folder_totals_reply(
        paths: &[String],
        deadline: Instant,
    ) -> Result<FolderTotalsReply, PipeError> {
        decode_totals_reply(query_totals(paths, deadline)?, paths.len())
    }
}

/// Exchanges a validated totals request, failing on transport errors or deadline expiry.
fn query_totals(paths: &[String], deadline: Instant) -> Result<Response, PipeError> {
    let request = Request {
        terms: vec![QueryTerm::FolderTotalsV1(paths.to_vec())],
    };
    if !request.within_limits() {
        return Err(PipeError::InvalidResponse);
    }
    let pipe = platform::Pipe::connect_before(PIPE_NAME, deadline)?;
    pipe.write(
        &Query {
            request,
            generation: 1,
        },
        Some(deadline),
    )?;
    pipe.read(Some(deadline), &Cancellation::Poll(&|| false))
}

/// Validates positional totals and rejects Busy for callers that require fallback.
fn decode_totals(response: Response, count: usize) -> Result<Vec<Option<FolderTotals>>, PipeError> {
    let reply = decode_totals_reply(response, count)?;
    if reply.busy {
        return Err(PipeError::InvalidResponse);
    }
    Ok(reply.totals)
}

/// Validates a totals response, preserving positional data and rebuild Busy state.
fn decode_totals_reply(response: Response, count: usize) -> Result<FolderTotalsReply, PipeError> {
    match response {
        Response::FolderTotalsV1 {
            generation: 1,
            reply,
        } if reply.totals.len() == count => Ok(reply),
        _ => Err(PipeError::InvalidResponse),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipe::{FolderTotalsReply, frames};

    #[test]
    fn total_components_have_one_shared_budget_across_all_paths() {
        let boundary = "C:".to_owned() + &"/a".repeat(4095);
        assert!(folder_paths_within_limits(std::slice::from_ref(&boundary)));
        assert!(!folder_paths_within_limits(&[
            boundary.clone(),
            "D:".into()
        ]));
        assert!(!folder_paths_within_limits(&[boundary + "/a"]));
    }

    #[test]
    fn busy_wrong_generations_and_wrong_counts_request_fallback() {
        for (generation, busy, count) in [(1, true, 1), (2, false, 1), (1, false, 0)] {
            let response = Response::FolderTotalsV1 {
                generation,
                reply: FolderTotalsReply {
                    totals: vec![None; count],
                    busy,
                },
            };
            let response = frames::decode(&frames::encode(&response).unwrap()).unwrap();
            assert!(decode_totals(response, 1).is_err());
        }
    }

    #[test]
    fn busy_reply_preserves_positions_and_still_validates_count_and_generation() {
        let reply = FolderTotalsReply {
            totals: vec![
                None,
                Some(FolderTotals {
                    bytes: 100,
                    complete: true,
                    ..Default::default()
                }),
            ],
            busy: true,
        };
        for (generation, count) in [(1, 2), (2, 2), (1, 1)] {
            let response = Response::FolderTotalsV1 {
                generation,
                reply: reply.clone(),
            };
            let response = frames::decode(&frames::encode(&response).unwrap()).unwrap();
            let decoded = decode_totals_reply(response, count);
            if generation == 1 && count == 2 {
                assert_eq!(decoded.ok(), Some(reply.clone()));
            } else {
                assert!(decoded.is_err());
            }
        }
    }
}
