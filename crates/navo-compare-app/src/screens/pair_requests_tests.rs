//! Tests for the Stream's pairs built from a folder session's rows.

use sidewise_ui::browser::{
    CompareControl, CompareKind, CompareRow, CompareSide, CompareUpdate, FolderSession, RowStatus,
};

use super::pair_requests::{neighbour, pair_request, stream_one, stream_request};

/// A compare control that ignores every request.
struct Idle;

impl CompareControl for Idle {
    /// Ignores a focus request.
    fn focus(&self, _row_ids: Vec<u64>) {}

    /// Ignores a stop request for one row.
    fn stop_row(&self, _id: u64) {}

    /// Ignores a stop request for the whole walk.
    fn stop_all(&self) {}

    /// Ignores a refresh request.
    fn refresh(&self) {}

    /// Ignores a request to resume a stopped walk.
    fn continue_walk(&self) {}
}

/// A file side of a size the fixture does not care about.
fn side() -> Option<CompareSide> {
    Some(CompareSide {
        kind: CompareKind::File,
        size: 1,
        modified_ms: None,
    })
}

/// One file row with its status and the sides its status implies.
fn row(id: u64, path: &str, status: RowStatus, sides: (bool, bool)) -> CompareRow {
    let left = if sides.0 { side() } else { None };
    let right = if sides.1 { side() } else { None };
    CompareRow::new(id, path, status, (left, right))
}

/// A session whose four file rows apply on generation 1.
fn session() -> FolderSession {
    let left = crate::screens::compare_run::location_of(&std::env::temp_dir().join("sw-left"))
        .expect("left root");
    let right = crate::screens::compare_run::location_of(&std::env::temp_dir().join("sw-right"))
        .expect("right root");
    let mut session = FolderSession::new(1, Box::new(Idle), [left, right]);
    let rows = vec![
        row(1, "a.txt", RowStatus::Different, (true, true)),
        row(2, "b.txt", RowStatus::Same, (true, true)),
        row(3, "c.txt", RowStatus::OnlyRight, (false, true)),
        row(4, "d.txt", RowStatus::Different, (true, true)),
    ];
    assert!(
        session.apply(CompareUpdate::Rows {
            generation: 1,
            rows,
            progress: Vec::new(),
        }),
        "the fixture rows apply"
    );
    session
}

/// A file diff's request keeps its id, path and both sides in the pair.
#[test]
fn pair_request_keeps_id_path_and_sides() {
    let session = session();
    let row = session
        .visible()
        .into_iter()
        .find(|row| row.id == 1)
        .expect("row 1");
    let pair = pair_request(session.file_diff(row));
    assert_eq!(pair.id, 1);
    assert_eq!(pair.path, "a.txt");
    assert!(pair.left.is_some());
    assert!(pair.right.is_some());
}

/// The Stream lists only the differing files, in row order, and keeps the right-only side.
#[test]
fn stream_request_lists_differences_in_order() {
    let (list, requests) = stream_request(&session());
    let ids: Vec<u64> = list.items.iter().map(|item| item.id).collect();
    assert_eq!(ids, [1, 3, 4]);
    assert_eq!(requests.len(), 3);
    let only_right = requests.iter().find(|pair| pair.id == 3).expect("pair 3");
    assert!(only_right.left.is_none());
}

/// A forward step skips the same file and lands on the next differing one.
#[test]
fn neighbour_steps_forward_over_same_files() {
    let pair = neighbour(&session(), (1, 1)).expect("next item");
    assert_eq!(pair.id, 3);
}

/// A backward step skips the same file and lands on the previous differing one.
#[test]
fn neighbour_steps_back() {
    let pair = neighbour(&session(), (4, -1)).expect("previous item");
    assert_eq!(pair.id, 3);
}

/// A step past either end of the Stream finds no pair.
#[test]
fn neighbour_stops_at_the_ends() {
    let session = session();
    assert!(neighbour(&session, (4, 1)).is_none());
    assert!(neighbour(&session, (1, -1)).is_none());
}

/// A one-card Stream holds the one row and its pair.
#[test]
fn stream_one_has_one_item() {
    let (list, requests) = stream_one(&session(), 4).expect("a differing file");
    let ids: Vec<u64> = list.items.iter().map(|item| item.id).collect();
    assert_eq!(ids, [4]);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].id, 4);
}

/// A same row is no Stream item, so it opens no Stream.
#[test]
fn stream_one_skips_a_same_row() {
    assert!(stream_one(&session(), 2).is_none());
    assert!(stream_one(&session(), 99).is_none());
}
