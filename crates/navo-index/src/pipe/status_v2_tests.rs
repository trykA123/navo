use super::*;
use crate::pipe::{
    PipeError, Query, QueryTerm, Request, Response, Status, VolumeStatus, frames,
    status::{Failure, read_status_v2},
};

fn volume(root: &str, entries: u64, total_files: Option<u64>) -> VolumeStatus {
    VolumeStatus {
        root: root.into(),
        entries,
        progress: Some(41),
        errors: vec!["x".into()],
        total_files,
    }
}

fn detail(state: VolumeState, total_files: Option<u64>) -> VolumeDetail {
    VolumeDetail {
        state,
        total_files,
        lag_bytes: 2 * 1024 * 1024,
        rebuild_in_s: Some(360),
        full_builds: 3,
        last_full_build_age_s: Some(10_800),
    }
}

#[test]
fn status_v2_round_trips_all_fields_and_volume_states() {
    let states = [
        VolumeState::Ready,
        VolumeState::Building,
        VolumeState::Updating,
        VolumeState::Behind,
        VolumeState::RebuildWaiting,
        VolumeState::Stale,
    ];
    let volumes: Vec<_> = states
        .iter()
        .enumerate()
        .map(|(idx, _)| {
            let total = (idx % 2 == 0).then_some(100 + idx as u64);
            volume(&format!("{}:", (b'C' + idx as u8) as char), 50, total)
        })
        .collect();
    let details: Vec<_> = states
        .iter()
        .enumerate()
        .map(|(idx, &state)| {
            let total = (idx % 2 == 0).then_some(100 + idx as u64);
            VolumeDetail {
                state,
                total_files: total,
                lag_bytes: (idx as u64) * 512 * 1024,
                rebuild_in_s: (state == VolumeState::RebuildWaiting).then_some(420),
                full_builds: idx as u64 + 1,
                last_full_build_age_s: (idx > 0).then_some(idx as u64 * 60),
            }
        })
        .collect();
    let response = Response::StatusV2 {
        generation: 7,
        status: Status { volumes },
        detail: details,
    };
    let encoded = frames::encode(&response).unwrap();
    let decoded: Response = frames::decode(&encoded).unwrap();
    assert_eq!(decoded, response);
}

#[test]
fn status_v2_refuses_detail_count_mismatch() {
    let response = Response::StatusV2 {
        generation: 7,
        status: Status {
            volumes: vec![volume("C:", 10, Some(20))],
        },
        detail: Vec::new(),
    };
    let encoded = frames::encode(&response).unwrap();
    assert!(
        frames::decode::<Response>(&encoded).is_err(),
        "detail.len() != status.volumes.len() must fail decode"
    );
}

#[test]
fn unknown_volume_state_code_decodes_as_updating() {
    let encoded = frames::encode(&99u8).unwrap();
    let decoded: VolumeState = frames::decode(&encoded).unwrap();
    assert_eq!(decoded, VolumeState::Updating);
}

#[test]
fn status_v2_request_stands_alone_and_carries_no_search_terms() {
    let alone = Request {
        terms: vec![QueryTerm::StatusV2],
    };
    assert!(alone.wants_status_v2() && alone.within_limits());
    assert!(alone.terms().is_empty());
    let mixed = Request {
        terms: vec![QueryTerm::StatusV2, QueryTerm::Name("x".into())],
    };
    assert!(!mixed.wants_status_v2() && !mixed.within_limits());
    let query = Query {
        request: alone,
        generation: 1,
    };
    let decoded: Query = frames::decode(&frames::encode(&query).unwrap()).unwrap();
    assert_eq!(decoded, query);
}

#[test]
fn client_falls_back_from_status_v2_to_status_v1_on_old_service() {
    let mut asked = Vec::new();
    let (status, details) = read_status_v2(|request, replies| {
        asked.push(request.terms.clone());
        assert_eq!(replies, 1);
        if request.wants_status_v2() {
            return Err(Failure::Exchange(PipeError::NotAvailable));
        }
        assert!(request.wants_status());
        Ok(vec![Response::StatusV1 {
            generation: 1,
            status: Status {
                volumes: vec![volume("C:", 12, Some(40))],
            },
        }])
    })
    .unwrap();
    assert_eq!(
        asked,
        vec![vec![QueryTerm::StatusV2], vec![QueryTerm::StatusV1]]
    );
    assert_eq!(status.volumes[0].entries, 12);
    assert_eq!(status.volumes[0].total_files, Some(40));
    assert!(details.is_none());
}

#[test]
fn client_reads_status_v2_in_one_exchange_from_new_service() {
    let mut asked = 0;
    let expected = vec![detail(VolumeState::Behind, Some(100))];
    let (status, details) = read_status_v2(|request, replies| {
        asked += 1;
        assert!(request.wants_status_v2() && replies == 1);
        let response = Response::StatusV2 {
            generation: 1,
            status: Status {
                volumes: vec![volume("C:", 50, Some(100))],
            },
            detail: expected.clone(),
        };
        Ok(vec![
            frames::decode(&frames::encode(&response).unwrap()).unwrap(),
        ])
    })
    .unwrap();
    assert_eq!(asked, 1);
    assert_eq!(status.volumes[0].total_files, Some(100));
    assert_eq!(details, Some(expected));
}
