//! The Stream's pairs for the compare tab: a file diff's pair, the Stream's items and their
//! pairs, and the pair beside a row. Adapted from Navo's compare entry.

use sidewise_ui::browser::{FileDiffRequest, FolderSession};
use sidewise_ui::compare_diff::{PairRequest, StreamList, is_stream_item, stream_items};

/// The file diff's form of a compare pair: the same id, path and sides.
pub fn pair_request(asked: FileDiffRequest) -> PairRequest {
    let FileDiffRequest {
        id,
        path,
        left,
        right,
    } = asked;
    PairRequest {
        id,
        path,
        left,
        right,
    }
}

/// The Stream's items under the session's folder and each item's pair; an item whose row has
/// not arrived gets a pair with no sides.
pub fn stream_request(session: &FolderSession) -> (StreamList, Vec<PairRequest>) {
    let list = stream_items(&session.rows_under_folder(), session.folder());
    let requests = list
        .items
        .iter()
        .map(|item| {
            session.pair_of(item.id).map_or_else(
                || PairRequest {
                    id: item.id,
                    path: item.path.clone(),
                    left: None,
                    right: None,
                },
                pair_request,
            )
        })
        .collect();
    (list, requests)
}

/// The pair of the next (`step` > 0) or previous Stream item after row `from`; `None` at either
/// end or when `from` is not under the folder.
pub fn neighbour(session: &FolderSession, (from, step): (u64, i32)) -> Option<PairRequest> {
    let found = session.row_beside(from, step, is_stream_item)?;
    session.pair_of(found).map(pair_request)
}

/// A Stream of the one row `id` and its pair; `None` unless the row is a Stream item.
pub fn stream_one(session: &FolderSession, id: u64) -> Option<(StreamList, Vec<PairRequest>)> {
    let (mut list, requests) = stream_request(session);
    let at = list.items.iter().position(|item| item.id == id)?;
    list.items = vec![list.items.swap_remove(at)];
    list.rules = 0;
    let pair = requests.into_iter().find(|pair| pair.id == id)?;
    Some((list, vec![pair]))
}
