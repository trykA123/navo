//! Exercises positional totals, rebuild Busy replies and deadline fallback.

use super::*;
use crate::answer::{SharedVolume, State};
use navo_index::{
    file::Header,
    ntfs::NtfsIndex,
    pipe::{CancelToken, QueryTerm, Request},
    table::{Flags, NO_PARENT, NameTable},
};

fn volume() -> SharedVolume {
    volume_at("C:")
}

/// Restores a full test generation rooted at the supplied drive.
fn volume_at(root: &str) -> SharedVolume {
    let mut table = NameTable::new();
    table.push(NO_PARENT, root, Flags::FOLDER).unwrap();
    table.push(0, "folder", Flags::FOLDER).unwrap();
    table.push(1, "file", Flags::NONE).unwrap();
    table.set_size(2, 100).unwrap();
    let header = Header {
        volume_serial: 1,
        journal_id: 2,
        next_usn: 10,
        root: root.into(),
        built_at: 0,
    };
    let index = NtfsIndex::restore((header, table, [(5, 0), (8, 1), (9, 2)].into()), 1, 2).unwrap();
    SharedVolume::new(State {
        index: Some(std::sync::Arc::new(index)),
        status: navo_index::pipe::VolumeStatus {
            root: root.into(),
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
    let reply = crate::answer::answer(std::slice::from_ref(&volume), &request, &CancelToken::new())
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
    let reply = answer_before(
        std::slice::from_ref(&volume),
        &paths,
        Instant::now() + Duration::from_secs(30),
    )
    .totals
    .unwrap();
    assert!(!reply.totals[0].as_ref().unwrap().complete);
    assert!(!reply.totals[1].as_ref().unwrap().complete);
    volume.shared.write().unwrap().index = None;
    let reply = answer_before(
        std::slice::from_ref(&volume),
        &paths,
        Instant::now() + Duration::from_secs(30),
    )
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
    assert!(!reply.busy);
    assert_eq!(reply.totals, [None]);
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn expired_deadlines_request_fallback_even_without_lock_contention() {
    let reply = answer_before(&[volume()], &["C:".into()], Instant::now())
        .totals
        .unwrap();
    assert!(!reply.busy);
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
    assert!(!reply.busy);
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

#[test]
fn search_only_totals_preserve_healthy_positions_and_search_hits() {
    let volumes = [volume(), volume_at("D:")];
    std::sync::Arc::get_mut(volumes[0].shared.write().unwrap().index.as_mut().unwrap())
        .unwrap()
        .make_search_only();
    let request = Request {
        terms: vec![QueryTerm::Name("file".into())],
    };
    let search = crate::answer::answer(&volumes, &request, &CancelToken::new());
    assert_eq!(search.hits.len(), 2);
    let paths = [
        "C:\\folder".into(),
        "D:\\folder".into(),
        "C:\\missing".into(),
    ];
    let reply = answer(&volumes, &paths).totals.unwrap();
    assert!(reply.busy, "requested search-only volumes must report Busy");
    assert_eq!(
        reply.totals,
        [
            None,
            Some(FolderTotals {
                bytes: 100,
                files: 1,
                folders: 0,
                complete: true
            }),
            None
        ]
    );
    let healthy = answer(&volumes, &["D:\\folder".into()]).totals.unwrap();
    assert!(!healthy.busy);
    assert_eq!(healthy.totals[0].as_ref().unwrap().bytes, 100);
}

#[test]
fn timeout_after_partial_mixed_work_erases_totals_and_busy() {
    let volumes = [volume(), volume_at("D:"), volume_at("E:")];
    std::sync::Arc::get_mut(volumes[0].shared.write().unwrap().index.as_mut().unwrap())
        .unwrap()
        .make_search_only();
    let _writer = volumes[2].shared.write().unwrap();
    let paths = [
        "C:\\folder".into(),
        "D:\\folder".into(),
        "E:\\folder".into(),
    ];
    let reply = answer_before(&volumes, &paths, Instant::now() + Duration::from_millis(20))
        .totals
        .unwrap();
    assert_eq!((reply.totals, reply.busy), (vec![None; 3], false));
}
