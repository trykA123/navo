use std::sync::{Mutex, mpsc};

use super::*;
use crate::pipe::session::{Transport, serve_session};
use crate::pipe::{CancelToken, Cancellation, Hit, MemoryReport, Query, Status, frames};

fn request(terms: Vec<QueryTerm>) -> Request {
    Request { terms }
}

fn known(revision: u32, build: &str) -> ServiceVersion {
    ServiceVersion::Known {
        revision,
        build: build.into(),
    }
}

fn answer(revision: u32, build: &str) -> Result<Vec<Response>, Failure> {
    Ok(vec![Response::VersionV1 {
        generation: 1,
        revision,
        build: build.into(),
    }])
}

#[test]
fn a_version_answer_round_trips() {
    let response = Response::VersionV1 {
        generation: 7,
        revision: 3,
        build: "abc123".into(),
    };
    let decoded: Response = frames::decode(&frames::encode(&response).unwrap()).unwrap();
    assert_eq!(decoded, response);
}

#[test]
fn version_frames_match_their_golden_bytes() {
    let term = Query {
        request: request(vec![QueryTerm::VersionV1]),
        generation: 1,
    };
    assert_eq!(frames::encode(&term).unwrap(), [3, 0, 0, 0, 1, 6, 1]);
    let response = Response::VersionV1 {
        generation: 7,
        revision: 1,
        build: "dev".into(),
    };
    let bytes = [7, 0, 0, 0, 5, 7, 1, 3, b'd', b'e', b'v'];
    assert_eq!(frames::encode(&response).unwrap(), bytes);
    assert_eq!(frames::decode::<Response>(&bytes).unwrap(), response);
}

#[test]
fn the_older_term_and_response_frames_keep_their_bytes() {
    let name = Query {
        request: request(vec![QueryTerm::Name("a".into())]),
        generation: 1,
    };
    assert_eq!(
        frames::encode(&name).unwrap(),
        [5, 0, 0, 0, 1, 0, 1, b'a', 1]
    );
    let memory = Query {
        request: request(vec![QueryTerm::MemoryV1]),
        generation: 1,
    };
    assert_eq!(frames::encode(&memory).unwrap(), [3, 0, 0, 0, 1, 4, 1]);
    let report = Response::MemoryV1 {
        generation: 7,
        report: MemoryReport::default(),
    };
    assert_eq!(frames::encode(&report).unwrap(), [4, 0, 0, 0, 3, 7, 0, 0]);
    let hits = Response::Hits {
        generation: 7,
        hits: vec![Hit {
            path: "a".into(),
            flags: 1,
            score: 2,
            indices: vec![],
        }],
    };
    assert_eq!(
        frames::encode(&hits).unwrap(),
        [8, 0, 0, 0, 0, 7, 1, 1, b'a', 1, 2, 0]
    );
}

#[test]
fn version_requests_stand_alone_and_carry_no_search_terms() {
    let alone = request(vec![QueryTerm::VersionV1]);
    assert!(alone.wants_version() && alone.within_limits());
    assert!(alone.terms().is_empty());
    let mixed = request(vec![QueryTerm::VersionV1, QueryTerm::Name("x".into())]);
    assert!(!mixed.wants_version() && !mixed.within_limits());
}

#[test]
fn a_running_service_gives_its_revision_and_build() {
    let version = read_version(|request| {
        assert!(request.wants_version());
        answer(4, "abc")
    });
    assert_eq!(version.unwrap(), known(4, "abc"));
}

#[derive(serde::Deserialize)]
#[expect(dead_code, reason = "only the decode result matters")]
enum OldTerm {
    Name(String),
    Path(String),
    Ext(Vec<String>),
    FolderTotalsV1(Vec<String>),
    MemoryV1,
    StatusV1,
}

#[derive(serde::Deserialize)]
#[expect(dead_code, reason = "only the decode result matters")]
struct OldQuery {
    terms: Vec<OldTerm>,
    generation: u64,
}

/// A service built before `VersionV1`: the unknown term fails the decode and drops the connection.
fn old_service(request: Request) -> Result<Vec<Response>, Failure> {
    let bytes = frames::encode(&Query {
        request,
        generation: 1,
    })
    .unwrap();
    frames::decode::<OldQuery>(&bytes).map_err(|_| Failure::Exchange(PipeError::NotAvailable))?;
    Ok(Vec::new())
}

#[test]
fn a_service_that_drops_the_unknown_term_is_older() {
    assert_eq!(read_version(old_service).unwrap(), ServiceVersion::Older);
}

#[test]
fn a_pipe_closed_after_the_request_or_an_undecodable_answer_is_older() {
    let failures: [fn() -> PipeError; 3] = [
        || PipeError::NotAvailable,
        || PipeError::Io(std::io::ErrorKind::BrokenPipe.into()),
        || PipeError::Frame(frames::FrameError::Truncated),
    ];
    for failure in failures {
        let version = read_version(|_| Err(Failure::Exchange(failure())));
        assert_eq!(version.unwrap(), ServiceVersion::Older);
    }
}

#[test]
fn no_pipe_or_a_timeout_is_an_error() {
    let failures: [fn() -> Failure; 3] = [
        || Failure::Connect(PipeError::NotAvailable),
        || Failure::Exchange(PipeError::TimedOut),
        || Failure::Exchange(PipeError::Cancelled),
    ];
    for failure in failures {
        assert!(read_version(|_| Err(failure())).is_err());
    }
}

#[test]
fn a_wrong_answer_is_refused() {
    let wrong = [
        vec![Response::VersionV1 {
            generation: 2,
            revision: 1,
            build: String::new(),
        }],
        vec![Response::StatusV1 {
            generation: 1,
            status: Status::default(),
        }],
        Vec::new(),
    ];
    for answer in wrong {
        assert!(matches!(
            read_version(|_| Ok(answer)),
            Err(PipeError::InvalidResponse)
        ));
    }
}

struct OnePipe {
    query: Mutex<Option<Query>>,
    responses: Mutex<Vec<Response>>,
    stop: CancelToken,
}

impl Transport for OnePipe {
    fn accept(&self, _: &Cancellation<'_>) -> Result<(), PipeError> {
        Ok(())
    }

    fn disconnect(&self) -> Result<(), PipeError> {
        Ok(())
    }

    fn read_query(&self, cancel: &Cancellation<'_>) -> Result<Query, PipeError> {
        if let Some(query) = self.query.lock().unwrap().take() {
            return Ok(query);
        }
        while !cancel.is_cancelled() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        Err(PipeError::Cancelled)
    }

    fn write_response(&self, response: &Response, _: std::time::Instant) -> Result<(), PipeError> {
        self.responses.lock().unwrap().push(response.clone());
        self.stop.cancel();
        Ok(())
    }
}

#[test]
fn the_session_answers_the_version_term_without_the_handler() {
    let stop = CancelToken::new();
    let pipe = OnePipe {
        query: Mutex::new(Some(Query {
            request: request(vec![QueryTerm::VersionV1]),
            generation: 9,
        })),
        responses: Mutex::new(Vec::new()),
        stop: stop.clone(),
    };
    let (answers, jobs) = mpsc::channel();
    drop(jobs);
    // The closed handler channel fails the test if the session sends a job.
    let _ = serve_session(&pipe, &answers, &[&stop]);
    assert_eq!(
        *pipe.responses.lock().unwrap(),
        [Response::VersionV1 {
            generation: 9,
            revision: SERVICE_REVISION,
            build: build_id().into(),
        }]
    );
}

#[test]
fn the_build_is_dev_without_a_build_id() {
    assert_eq!(build_id(), option_env!("NAVO_BUILD").unwrap_or("dev"));
}
