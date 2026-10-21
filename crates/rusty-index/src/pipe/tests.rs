use super::frames::{self, FrameError, MAX_FRAME_BYTES};
use super::{Hit, Query, QueryTerm, Request, Response, Status, VolumeStatus};

mod clients;

fn query(generation: u64) -> Query {
    Query {
        request: Request {
            terms: vec![
                QueryTerm::Name("résumé".into()),
                QueryTerm::Path(r"C:\work".into()),
                QueryTerm::Ext(vec!["rs".into(), "toml".into()]),
            ],
        },
        generation,
    }
}

#[test]
fn frames_round_trip() {
    let query = query(42);
    let encoded = frames::encode(&query).unwrap();
    assert_eq!(frames::decode::<Query>(&encoded).unwrap(), query);
    let responses = [
        Response::Hits {
            generation: 42,
            hits: vec![Hit {
                path: r"C:\work\résumé.rs".into(),
                flags: 3,
                score: 123,
                indices: vec![0, 1, 2],
            }],
        },
        Response::Status {
            generation: 42,
            status: Status {
                volumes: vec![VolumeStatus {
                    root: r"C:\".into(),
                    entries: 1234,
                    progress: Some(41),
                    errors: vec!["volume unavailable".into()],
                }],
            },
        },
    ];
    for response in responses {
        let encoded = frames::encode(&response).unwrap();
        assert_eq!(frames::decode::<Response>(&encoded).unwrap(), response);
    }
}

#[test]
fn frames_refuse_oversized_input_and_output() {
    let oversized = (MAX_FRAME_BYTES as u32 + 1).to_le_bytes();
    assert!(matches!(
        frames::decode::<Query>(&oversized),
        Err(FrameError::TooLarge)
    ));
    assert!(matches!(
        frames::decode::<Query>(&u32::MAX.to_le_bytes()),
        Err(FrameError::TooLarge)
    ));
    let text = "x".repeat(MAX_FRAME_BYTES);
    assert!(matches!(frames::encode(&text), Err(FrameError::TooLarge)));
}

#[test]
fn frames_refuse_truncated_prefix_and_payload() {
    let encoded = frames::encode(&query(1)).unwrap();
    for length in 0..encoded.len() {
        assert!(matches!(
            frames::decode::<Query>(&encoded[..length]),
            Err(FrameError::Truncated)
        ));
    }
}

#[cfg(not(windows))]
#[test]
fn unix_stub_reports_not_available() {
    use super::{Client, PipeError, Server};
    assert!(matches!(Client::connect(), Err(PipeError::NotAvailable)));
    assert!(matches!(
        Server::bind("S-1-5-21-1-2-3-1000"),
        Err(PipeError::NotAvailable)
    ));
    let mut client = Client {
        pipe: None,
        pending: None,
    };
    assert!(matches!(
        client.query(&query(1)),
        Err(PipeError::NotAvailable)
    ));
    let mut client = Client {
        pipe: Some(super::unix::Pipe),
        pending: Some((1, std::time::Instant::now())),
    };
    assert!(matches!(
        client.receive(&|| false),
        Err(PipeError::NotAvailable)
    ));
    let mut server = Server {
        pipes: vec![super::unix::Pipe],
    };
    assert!(matches!(
        server.serve(&|_, _| super::Reply::default(), &super::CancelToken::new()),
        Err(PipeError::NotAvailable)
    ));
}

#[test]
fn newer_generation_cancels_the_running_search_and_keeps_only_the_latest() {
    let mut pending = super::Pending::default();
    pending.accept(query(1));
    let running = pending.cancel.clone();
    assert_eq!(pending.query.take().unwrap().generation, 1);
    pending.accept(query(2));
    assert!(running.is_cancelled());
    let second = pending.cancel.clone();
    pending.accept(query(3));
    assert!(second.is_cancelled());
    pending.accept(query(2));
    pending.accept(query(3));
    assert!(!pending.cancel.is_cancelled());
    assert_eq!(pending.query.take().unwrap().generation, 3);
}

#[cfg(windows)]
mod windows;

#[test]
fn oversized_hits_drop_from_the_tail_using_the_wire_size() {
    let mut reply = super::Reply {
        hits: (1..=3)
            .map(|score| Hit {
                path: "x".repeat(MAX_FRAME_BYTES / 2 - 16),
                flags: 0,
                score,
                indices: Vec::new(),
            })
            .collect(),
        status: Status::default(),
    };
    let response = super::prepare_hits(u64::MAX, &mut reply).unwrap();
    let encoded = frames::encode(&response).unwrap();
    let Response::Hits { hits, .. } = response else {
        panic!("expected hits");
    };
    assert_eq!(hits.iter().map(|hit| hit.score).collect::<Vec<_>>(), [3, 2]);
    assert!(encoded.len() <= MAX_FRAME_BYTES + 4);
    reply.hits = hits;
    for hit in &mut reply.hits {
        hit.path = "x".repeat(MAX_FRAME_BYTES / 2 - 4);
    }
    let response = super::prepare_hits(u64::MAX, &mut reply).unwrap();
    assert!(matches!(response, Response::Hits { hits, .. } if hits.len() == 1));
    reply.hits = vec![Hit {
        path: "x".repeat(MAX_FRAME_BYTES),
        flags: 0,
        score: 1,
        indices: Vec::new(),
    }];
    let response = super::prepare_hits(1, &mut reply).unwrap();
    assert!(matches!(response, Response::Hits { hits, .. } if hits.is_empty()));
}

#[test]
fn oversized_requests_return_empty_hits_and_an_error_without_running_the_handler() {
    let invalid = [
        vec![QueryTerm::Name(String::new()); 65],
        vec![QueryTerm::Name("x".repeat(1025))],
        vec![QueryTerm::Path("é".repeat(513))],
        vec![QueryTerm::Ext(vec!["x".repeat(513); 2])],
        vec![QueryTerm::Ext(vec![String::new(); 1026])],
    ];
    for terms in invalid {
        let reply = super::answer_request(
            &Request { terms },
            &|_, _| panic!("invalid request reached the handler"),
            &super::CancelToken::new(),
        );
        assert!(reply.hits.is_empty());
        assert!(
            reply
                .status
                .volumes
                .iter()
                .any(|volume| !volume.errors.is_empty())
        );
    }
    let request = Request {
        terms: vec![QueryTerm::Name("x".repeat(1024)); 64],
    };
    let called = std::sync::atomic::AtomicBool::new(false);
    super::answer_request(
        &request,
        &|_, _| {
            called.store(true, std::sync::atomic::Ordering::Relaxed);
            super::Reply::default()
        },
        &super::CancelToken::new(),
    );
    assert!(called.load(std::sync::atomic::Ordering::Relaxed));
}
