use super::*;
use crate::index_service::answer::{SharedVolume, State};
use rusty_index::{
    file::Header,
    ntfs::NtfsIndex,
    pipe::{CancelToken, QueryTerm, Request},
    table::{Flags, NO_PARENT, NameTable},
};

fn volume() -> SharedVolume {
    let mut table = NameTable::new();
    table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    table.push(0, "folder", Flags::FOLDER).unwrap();
    table.push(1, "file", Flags::NONE).unwrap();
    table.set_size(2, 100).unwrap();
    let header = Header {
        volume_serial: 1,
        journal_id: 2,
        next_usn: 10,
        root: "C:".into(),
        built_at: 0,
    };
    let index = NtfsIndex::restore((header, table, [(5, 0), (8, 1), (9, 2)].into()), 1, 2).unwrap();
    SharedVolume::new(State {
        index: Some(std::sync::Arc::new(index)),
        status: rusty_index::pipe::VolumeStatus {
            root: "C:".into(),
            ..Default::default()
        },
        ..Default::default()
    })
}

#[test]
fn service_answers_requested_totals_and_marks_building_volumes_incomplete() {
    let volume = volume();
    let request = Request {
        terms: vec![QueryTerm::FolderTotalsV1(vec![
            "c:/folder/".into(),
            "C:\\missing".into(),
            "D:\\".into(),
        ])],
    };
    let reply = crate::index_service::answer::answer(
        std::slice::from_ref(&volume),
        &request,
        &CancelToken::new(),
    )
    .totals
    .unwrap();
    assert!(!reply.busy);
    assert_eq!(
        reply.totals,
        [
            Some(FolderTotals {
                bytes: 100,
                files: 1,
                folders: 0,
                complete: true
            }),
            None,
            None
        ]
    );
}

#[test]
fn building_volumes_report_incomplete_totals() {
    let volume = volume();
    let paths = ["C:\\folder".into(), "C:\\missing".into()];
    volume.shared.write().unwrap().status.progress = Some(0);
    let reply = answer(std::slice::from_ref(&volume), &paths)
        .totals
        .unwrap();
    assert!(!reply.totals[0].as_ref().unwrap().complete);
    assert!(!reply.totals[1].as_ref().unwrap().complete);
    volume.shared.write().unwrap().index = None;
    let reply = answer(std::slice::from_ref(&volume), &paths)
        .totals
        .unwrap();
    assert_eq!(reply.totals[0], Some(FolderTotals::default()));
}

#[test]
fn busy_volume_answers_within_budget_and_requests_fallback() {
    let volume = volume();
    let _writer = volume.shared.write().unwrap();
    let started = Instant::now();
    let reply = answer(std::slice::from_ref(&volume), &["C:\\folder".into()])
        .totals
        .unwrap();
    assert!(reply.busy);
    assert!(started.elapsed() < Duration::from_millis(50));
}

#[test]
fn expired_deadlines_request_fallback_even_without_lock_contention() {
    let reply = answer_before(&[volume()], &["C:".into()], Instant::now())
        .totals
        .unwrap();
    assert!(reply.busy);
    assert_eq!(reply.totals, [None]);
}

#[test]
fn total_path_components_are_capped_before_taking_the_read_lock() {
    let volume = volume();
    let _writer = volume.shared.write().unwrap();
    let paths = vec!["C:/".to_owned() + &"a/".repeat(64); 64];
    let reply = answer_before(
        std::slice::from_ref(&volume),
        &paths,
        Instant::now() + Duration::from_secs(60),
    )
    .totals
    .unwrap();
    assert!(reply.busy);
    assert!(reply.totals.is_empty());
}

#[test]
fn service_folder_paths_match_unicode_names() {
    let volume = volume();
    std::sync::Arc::make_mut(volume.shared.write().unwrap().index.as_mut().unwrap())
        .table
        .rename(1, 0, "Ä")
        .unwrap();
    let reply = answer(&[volume], &["c:/ä".into()]).totals.unwrap();
    assert!(!reply.busy);
    assert_eq!(reply.totals[0].as_ref().unwrap().bytes, 100);
}
