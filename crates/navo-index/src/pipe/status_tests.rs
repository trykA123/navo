use super::*;
use crate::pipe::{Hit, frames};

fn volume(entries: u64, total_files: Option<u64>) -> VolumeStatus {
    VolumeStatus {
        root: "C:".into(),
        entries,
        progress: Some(41),
        errors: vec!["x".into()],
        total_files,
    }
}

fn detailed(volumes: Vec<VolumeStatus>) -> Response {
    Response::StatusV1 {
        generation: 7,
        status: Status { volumes },
    }
}

const LEGACY_STATUS: [u8; 16] = [12, 0, 0, 0, 1, 7, 1, 2, b'C', b':', 3, 1, 41, 1, 1, b'x'];
const DETAILED_STATUS: [u8; 19] = [
    15, 0, 0, 0, 4, 7, 1, 2, b'C', b':', 3, 1, 41, 1, 1, b'x', 1, 1, 100,
];

#[test]
fn a_detailed_status_round_trips_with_known_and_unknown_totals() {
    let response = detailed(vec![volume(3, Some(9)), volume(0, None)]);
    let decoded: Response = frames::decode(&frames::encode(&response).unwrap()).unwrap();
    assert_eq!(decoded, response);
}

#[test]
fn a_detailed_status_matches_its_golden_bytes() {
    let response = detailed(vec![volume(3, Some(100))]);
    assert_eq!(frames::encode(&response).unwrap(), DETAILED_STATUS);
    assert_eq!(
        frames::decode::<Response>(&DETAILED_STATUS).unwrap(),
        response
    );
    assert_eq!(DETAILED_STATUS[5..16], LEGACY_STATUS[5..]);
}

#[test]
fn the_legacy_status_frame_keeps_its_old_bytes_and_drops_the_total() {
    let legacy = Response::Status {
        generation: 7,
        status: Status {
            volumes: vec![volume(3, Some(200))],
        },
    };
    assert_eq!(frames::encode(&legacy).unwrap(), LEGACY_STATUS);
    let Response::Status { status, .. } = frames::decode(&LEGACY_STATUS).unwrap() else {
        panic!("the old status frame must decode as a status");
    };
    assert_eq!(status.volumes[0].total_files, None);
}

#[test]
fn a_decoded_total_below_the_entries_is_raised_to_them() {
    let mut bytes = DETAILED_STATUS.to_vec();
    bytes[18] = 2;
    let Response::StatusV1 { status, .. } = frames::decode(&bytes).unwrap() else {
        panic!("a detailed status frame must decode as one");
    };
    assert_eq!(status.volumes[0].total_files, Some(3));
}

#[test]
fn totals_that_do_not_match_the_volumes_are_refused() {
    let mut bytes = DETAILED_STATUS.to_vec();
    bytes.truncate(17);
    bytes[16] = 0;
    bytes[0] = 13;
    assert!(frames::decode::<Response>(&bytes).is_err());
}

#[test]
fn clamping_keeps_an_unknown_total_unknown() {
    let mut status = volume(10, None);
    status.clamp_total();
    assert_eq!(status.total_files, None);
    let mut status = volume(10, Some(4));
    status.clamp_total();
    assert_eq!(status.total_files, Some(10));
}

#[test]
fn status_requests_stand_alone_and_carry_no_search_terms() {
    let alone = Request {
        terms: vec![QueryTerm::StatusV1],
    };
    assert!(alone.wants_status() && alone.within_limits());
    assert!(alone.terms().is_empty());
    let mixed = Request {
        terms: vec![QueryTerm::StatusV1, QueryTerm::Name("x".into())],
    };
    assert!(!mixed.wants_status() && !mixed.within_limits());
    let query = Query {
        request: alone,
        generation: 1,
    };
    assert_eq!(frames::encode(&query).unwrap(), [3, 0, 0, 0, 1, 5, 1]);
}

#[derive(serde::Deserialize)]
#[expect(dead_code, reason = "only the decode result matters")]
enum OldTerm {
    Name(String),
    Path(String),
    Ext(Vec<String>),
    FolderTotalsV1(Vec<String>),
    MemoryV1,
}

#[derive(serde::Deserialize)]
struct OldQuery {
    terms: Vec<OldTerm>,
    generation: u64,
}

#[derive(serde::Serialize)]
struct OldVolume {
    root: String,
    entries: u64,
    progress: Option<u8>,
    errors: Vec<String>,
}

#[derive(serde::Serialize)]
enum OldResponse {
    Hits {
        generation: u64,
        hits: Vec<Hit>,
    },
    Status {
        generation: u64,
        volumes: Vec<OldVolume>,
    },
}

/// Answers like a service built before `StatusV1`: an unknown term fails the decode and drops the connection.
fn old_service(request: Request, replies: usize) -> Result<Vec<Response>, Failure> {
    let bytes = frames::encode(&Query {
        request,
        generation: 1,
    })
    .unwrap();
    let query: OldQuery =
        frames::decode(&bytes).map_err(|_| Failure::Exchange(PipeError::NotAvailable))?;
    assert_eq!((query.terms.len(), replies), (0, 2));
    let status = OldResponse::Status {
        generation: query.generation,
        volumes: vec![OldVolume {
            root: "C:".into(),
            entries: 3,
            progress: Some(0),
            errors: Vec::new(),
        }],
    };
    let hits = OldResponse::Hits {
        generation: query.generation,
        hits: Vec::new(),
    };
    [hits, status]
        .iter()
        .map(|response| Ok(frames::decode(&frames::encode(response).unwrap()).unwrap()))
        .collect()
}

#[test]
fn an_old_service_gives_the_status_without_totals() {
    let mut asked = 0;
    let status = read_status(|request, replies| {
        asked += 1;
        old_service(request, replies)
    })
    .unwrap();
    assert_eq!(asked, 2);
    assert_eq!(status.volumes[0].entries, 3);
    assert_eq!(status.volumes[0].total_files, None);
}

#[test]
fn a_new_service_answers_the_detailed_status_in_one_exchange() {
    let mut asked = 0;
    let status = read_status(|request, replies| {
        asked += 1;
        assert!(request.wants_status() && replies == 1);
        let response = Response::StatusV1 {
            generation: 1,
            status: Status {
                volumes: vec![volume(3, Some(9))],
            },
        };
        Ok(vec![
            frames::decode(&frames::encode(&response).unwrap()).unwrap(),
        ])
    })
    .unwrap();
    assert_eq!(asked, 1);
    assert_eq!(status.volumes[0].total_files, Some(9));
}

#[test]
fn a_missing_service_or_a_timeout_is_not_asked_twice() {
    let failures: [fn() -> Failure; 2] = [
        || Failure::Connect(PipeError::NotAvailable),
        || Failure::Exchange(PipeError::TimedOut),
    ];
    for failure in failures {
        let mut asked = 0;
        let result = read_status(|_, _| {
            asked += 1;
            Err(failure())
        });
        assert!(result.is_err());
        assert_eq!(asked, 1);
    }
}

#[test]
fn a_wrong_detailed_answer_is_refused() {
    let wrong = || {
        Ok(vec![Response::StatusV1 {
            generation: 2,
            status: Status::default(),
        }])
    };
    assert!(matches!(
        read_status(|_, _| wrong()),
        Err(PipeError::InvalidResponse)
    ));
}
