use super::*;
use crate::pipe::{Status, VolumeStatus, frames};
use crate::table::Flags;
use crate::usn::{JournalPosition, UsnBatch, UsnRecord};

const POSITION: JournalPosition = JournalPosition {
    journal_id: 7,
    next_usn: 100,
};

fn record(file_id: u128, reason: u32, name: &str) -> UsnRecord {
    UsnRecord {
        file_id,
        parent_id: 5,
        usn: 90,
        reason,
        flags: Flags::NONE,
        name: name.into(),
        size: Some(10),
    }
}

fn index(files: u128) -> NtfsIndex {
    let mut index = NtfsIndex::new('C', POSITION).unwrap();
    let mut records: Vec<_> = (0..files)
        .map(|at| record(100 + at, 0x100, &format!("file-{at}.txt")))
        .collect();
    records.push(record(99, 0x1000, "moving.txt"));
    index
        .apply_batch(UsnBatch {
            next_usn: 110,
            records,
        })
        .unwrap();
    index
}

fn part(report: &MemoryReport, wanted: Part) -> MemoryPart {
    report
        .known()
        .find(|(found, _)| *found == wanted)
        .map(|(_, found)| found)
        .unwrap_or_default()
}

fn report(files: u128) -> MemoryReport {
    let mut report = MemoryReport::default();
    report.add_index(&index(files));
    report
}

#[test]
fn an_index_with_entries_reports_every_index_part() {
    let report = report(50);
    for wanted in [Part::Names, Part::FileIds, Part::Tree, Part::Totals] {
        let found = part(&report, wanted);
        assert!(found.bytes > 0, "{wanted:?} has no bytes");
        assert!(found.items > 0, "{wanted:?} has no items");
    }
    assert_eq!(part(&report, Part::Names).items, 51);
    assert_eq!(part(&report, Part::FileIds).items, 51);
    assert!(part(&report, Part::Journal).bytes > 0);
}

#[test]
fn index_parts_grow_with_the_index() {
    let mut last = report(0);
    for files in [10, 100, 1_000, 10_000] {
        let next = report(files);
        for wanted in [Part::Names, Part::FileIds, Part::Tree, Part::Totals] {
            assert!(
                part(&next, wanted).bytes > part(&last, wanted).bytes,
                "{wanted:?} at {files}"
            );
        }
        last = next;
    }
}

#[test]
fn two_volumes_add_up_in_one_row_per_part() {
    let mut both = MemoryReport::default();
    both.add_index(&index(10));
    both.add_index(&index(20));
    let names = part(&both, Part::Names);
    assert_eq!(names.items, 11 + 21);
    assert_eq!(
        names.bytes,
        part(&report(10), Part::Names).bytes + part(&report(20), Part::Names).bytes
    );
    assert_eq!(both.parts.len(), 5);
}

#[test]
fn codes_and_labels_are_stable_and_unknown_codes_are_skipped() {
    let codes: Vec<_> = PARTS.iter().map(|(part, _)| part.code()).collect();
    assert_eq!(codes, [0, 1, 2, 3, 4, 5, 6]);
    for (part, label) in PARTS {
        assert_eq!(Part::from_code(part.code()), Some(part));
        assert_eq!(part.label(), label);
        assert!(label.starts_with("index service "));
    }
    let report = MemoryReport {
        parts: vec![
            MemoryPart {
                part: 900,
                bytes: 1,
                items: 1,
            },
            MemoryPart {
                part: Part::Process.code(),
                bytes: 2,
                items: 0,
            },
        ],
        busy: false,
    };
    let known: Vec<_> = report.known().map(|(part, _)| part).collect();
    assert_eq!(known, [Part::Process]);
}

#[test]
fn a_memory_answer_round_trips_on_the_wire() {
    let mut report = report(5);
    report.add(Part::Process, 64 << 20, 0);
    report.add_sessions();
    report.busy = true;
    let response = Response::MemoryV1 {
        generation: 1,
        report: report.clone(),
    };
    let decoded = frames::decode(&frames::encode(&response).unwrap()).unwrap();
    assert_eq!(decode_memory(decoded).unwrap(), report);
}

#[test]
fn other_answers_or_generations_are_not_a_memory_report() {
    for response in [
        Response::MemoryV1 {
            generation: 2,
            report: MemoryReport::default(),
        },
        Response::Status {
            generation: 1,
            status: Status::default(),
        },
    ] {
        assert!(matches!(
            decode_memory(response),
            Err(PipeError::InvalidResponse)
        ));
    }
}

#[test]
fn memory_requests_stand_alone_and_carry_no_search_terms() {
    let alone = Request {
        terms: vec![QueryTerm::MemoryV1],
    };
    assert!(alone.wants_memory());
    assert!(alone.within_limits());
    assert!(alone.terms().is_empty());
    let mixed = Request {
        terms: vec![QueryTerm::MemoryV1, QueryTerm::Name("x".into())],
    };
    assert!(!mixed.wants_memory());
    assert!(!mixed.within_limits());
}

#[test]
fn an_old_status_frame_still_decodes() {
    let old = [12, 0, 0, 0, 1, 7, 1, 2, b'C', b':', 3, 1, 41, 1, 1, b'x'];
    let expected = Response::Status {
        generation: 7,
        status: Status {
            volumes: vec![VolumeStatus {
                root: "C:".into(),
                entries: 3,
                progress: Some(41),
                errors: vec!["x".into()],
            }],
        },
    };
    assert_eq!(frames::decode::<Response>(&old).unwrap(), expected);
    assert_eq!(frames::encode(&expected).unwrap(), old);
}

#[derive(serde::Deserialize)]
#[expect(dead_code, reason = "only the decode result matters")]
enum OldTerm {
    Name(String),
    Path(String),
    Ext(Vec<String>),
    FolderTotalsV1(Vec<String>),
}

#[derive(serde::Deserialize)]
#[expect(dead_code, reason = "only the decode result matters")]
struct OldQuery {
    terms: Vec<OldTerm>,
    generation: u64,
}

#[test]
fn an_old_service_refuses_a_memory_request_instead_of_misreading_it() {
    let query = Query {
        request: Request {
            terms: vec![QueryTerm::MemoryV1],
        },
        generation: 1,
    };
    let bytes = frames::encode(&query).unwrap();
    assert!(frames::decode::<OldQuery>(&bytes).is_err());
}
