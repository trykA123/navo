use std::time::Instant;

use super::{
    Cancellation, Client, FolderTotals, PIPE_NAME, PipeError, Query, QueryTerm, Request, Response,
    platform,
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
    pub fn folder_totals(
        paths: &[String],
        deadline: Instant,
    ) -> Result<Vec<Option<FolderTotals>>, PipeError> {
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
        let response = pipe.read(Some(deadline), &Cancellation::Poll(&|| false))?;
        decode_totals(response, paths.len())
    }
}

fn decode_totals(response: Response, count: usize) -> Result<Vec<Option<FolderTotals>>, PipeError> {
    match response {
        Response::FolderTotalsV1 {
            generation: 1,
            reply,
        } if !reply.busy && reply.totals.len() == count => Ok(reply.totals),
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
}
