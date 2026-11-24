//! The index service as the browser's folder-totals source: one pipe request per chunk of
//! shown folders, cut to the pipe limits (64 paths, 4,096 path components).

use std::sync::Arc;
use std::time::{Duration, Instant};

use navo_core::Location;
use navo_fs::CancelToken;
use navo_index::pipe::{Client, FolderTotals, FolderTotalsReply, folder_paths_within_limits};
use navo_ui::browser::{FolderTotalsQuery, FolderTotalsSource, TotalsState};

fn service_reply(paths: &[String]) -> Option<FolderTotalsReply> {
    Client::folder_totals_reply(paths, Instant::now() + Duration::from_millis(40)).ok()
}

/// The index service as a totals source; the reply function is injected so tests need no pipe.
struct ServiceTotals {
    reply: fn(&[String]) -> Option<FolderTotalsReply>,
}

impl FolderTotalsSource for ServiceTotals {
    fn query(&self, folder: &Location) -> TotalsState {
        if !folder.is_local() {
            return TotalsState::Unavailable;
        }
        let Some(reply) = (self.reply)(&[folder.as_str().to_owned()]) else {
            return TotalsState::Unavailable;
        };
        state(reply.totals.into_iter().next().flatten(), reply.busy)
    }

    /// Asks in as few requests as the pipe limits allow, one chunk after another; a failed
    /// request makes only its own chunk `Unavailable`. Once `cancel` fires, no further chunk is
    /// sent and the rest stay `Unavailable`. Keeps the input's length and order.
    fn query_many(&self, folders: &[Location], cancel: &CancelToken) -> Vec<TotalsState> {
        let mut answers = vec![TotalsState::Unavailable; folders.len()];
        let mut requests = 0;
        for chunk in chunks(folders) {
            if cancel.is_cancelled() {
                break;
            }
            let paths: Vec<String> = chunk
                .iter()
                .map(|&i| folders[i].as_str().to_owned())
                .collect();
            requests += 1;
            let Some(reply) = (self.reply)(&paths) else {
                continue;
            };
            let mut totals = reply.totals.into_iter();
            for &index in &chunk {
                answers[index] = state(totals.next().flatten(), reply.busy);
            }
        }
        log_counts(&answers, requests);
        answers
    }
}

/// Turns one positional reply entry into a state: totals are `Ready`, none while busy is `Busy`.
fn state(totals: Option<FolderTotals>, busy: bool) -> TotalsState {
    match totals {
        Some(totals) => TotalsState::Ready(totals),
        None if busy => TotalsState::Busy,
        None => TotalsState::Unavailable,
    }
}

/// Cuts the local folders into chunks of input indexes, in order, each within the pipe limits.
/// A folder that is not local or fails the limits alone joins no chunk.
fn chunks(folders: &[Location]) -> Vec<Vec<usize>> {
    let mut done = Vec::new();
    let mut indexes = Vec::new();
    let mut paths: Vec<String> = Vec::new();
    for (index, folder) in folders.iter().enumerate() {
        let path = folder.as_str().to_owned();
        if !folder.is_local() || !folder_paths_within_limits(std::slice::from_ref(&path)) {
            continue;
        }
        paths.push(path);
        if !folder_paths_within_limits(&paths) {
            let last = paths.pop().into_iter().collect();
            done.push(std::mem::take(&mut indexes));
            paths = last;
        }
        indexes.push(index);
    }
    if !indexes.is_empty() {
        done.push(indexes);
    }
    done
}

/// Logs the one count line NV14 reads: folders, requests and answers by state, never paths.
fn log_counts(answers: &[TotalsState], requests: usize) {
    tracing::info!("{}", count_line(answers, requests));
}

/// Builds the count line from the answers and the number of pipe requests; pure, so tests read it.
fn count_line(answers: &[TotalsState], requests: usize) -> String {
    let count = |want: fn(&TotalsState) -> bool| answers.iter().filter(|a| want(a)).count();
    format!(
        "folder totals: folders={} requests={requests} complete={} busy={} unavailable={} incomplete={}",
        answers.len(),
        count(|a| matches!(a, TotalsState::Ready(t) if t.complete)),
        count(|a| matches!(a, TotalsState::Busy)),
        count(|a| matches!(a, TotalsState::Unavailable)),
        count(|a| matches!(a, TotalsState::Ready(t) if !t.complete)),
    )
}

pub fn query() -> FolderTotalsQuery {
    Arc::new(ServiceTotals {
        reply: service_reply,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder() -> Location {
        Location::new(r"C:\proj").unwrap()
    }

    fn totals() -> FolderTotals {
        FolderTotals {
            bytes: 4096,
            files: 3,
            folders: 1,
            complete: true,
        }
    }

    fn rebuilding(_: &[String]) -> Option<FolderTotalsReply> {
        Some(FolderTotalsReply {
            totals: vec![None],
            busy: true,
        })
    }

    fn expired_deadline(_: &[String]) -> Option<FolderTotalsReply> {
        Some(FolderTotalsReply {
            totals: vec![None],
            busy: false,
        })
    }

    fn complete_reply(paths: &[String]) -> Option<FolderTotalsReply> {
        assert_eq!(
            paths,
            [r"C:\proj".to_owned()].as_slice(),
            "the source asks for the one folder it was given"
        );
        Some(FolderTotalsReply {
            totals: vec![Some(totals())],
            busy: false,
        })
    }

    fn transport_failure(_: &[String]) -> Option<FolderTotalsReply> {
        None
    }

    fn never_asked(_: &[String]) -> Option<FolderTotalsReply> {
        panic!("a non-local folder must not reach the index service");
    }

    #[test]
    fn rebuilding_reply_stays_busy() {
        assert_eq!(
            ServiceTotals { reply: rebuilding }.query(&folder()),
            TotalsState::Busy,
            "a rebuilding volume must stay Busy so a row waits instead of falling back"
        );
    }

    #[test]
    fn matching_total_is_ready() {
        assert_eq!(
            ServiceTotals {
                reply: complete_reply
            }
            .query(&folder()),
            TotalsState::Ready(totals())
        );
    }

    #[test]
    fn expired_deadline_reply_is_unavailable() {
        assert_eq!(
            ServiceTotals {
                reply: expired_deadline
            }
            .query(&folder()),
            TotalsState::Unavailable,
            "a service deadline expiry must not leave the row waiting forever"
        );
    }

    #[test]
    fn transport_failure_is_unavailable() {
        assert_eq!(
            ServiceTotals {
                reply: transport_failure
            }
            .query(&folder()),
            TotalsState::Unavailable
        );
    }

    #[test]
    fn non_local_folder_is_unavailable_without_asking() {
        let network = Location::new("ftp://host/pub").unwrap();
        assert_eq!(
            ServiceTotals { reply: never_asked }.query(&network),
            TotalsState::Unavailable
        );
    }

    #[test]
    fn oversized_request_returns_none_before_connecting() {
        assert!(super::service_reply(&vec!["C:".into(); 65]).is_none());
    }
    #[cfg(not(windows))]
    #[test]
    fn absent_service_returns_none_within_budget() {
        let started = std::time::Instant::now();
        assert!(super::service_reply(&["C:\\folder".into()]).is_none());
        assert!(started.elapsed() < std::time::Duration::from_millis(50));
    }

    thread_local! {
        static SENT: std::cell::RefCell<Vec<Vec<String>>> = const { std::cell::RefCell::new(Vec::new()) };
        static STOP: CancelToken = CancelToken::new();
    }

    /// Records each request and answers Ready, with `files` set to the path's trailing number.
    fn recording(paths: &[String]) -> Option<FolderTotalsReply> {
        SENT.with(|sent| sent.borrow_mut().push(paths.to_vec()));
        let numbered = |path: &String| FolderTotals {
            files: path
                .rsplit('\\')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0),
            ..totals()
        };
        Some(FolderTotalsReply {
            totals: paths.iter().map(|path| Some(numbered(path))).collect(),
            busy: false,
        })
    }

    /// Fails every request whose first path ends in `\\0`; other requests are recorded and Ready.
    fn first_chunk_fails(paths: &[String]) -> Option<FolderTotalsReply> {
        if paths[0].ends_with("\\0") {
            SENT.with(|sent| sent.borrow_mut().push(paths.to_vec()));
            return None;
        }
        recording(paths)
    }

    /// Records the request, then fires `STOP`, as a user leaving the folder mid-batch does.
    fn leave_after_reply(paths: &[String]) -> Option<FolderTotalsReply> {
        STOP.with(CancelToken::cancel);
        recording(paths)
    }

    /// Answers no totals with the busy flag set, as a rebuilding volume does.
    fn all_busy(paths: &[String]) -> Option<FolderTotalsReply> {
        Some(FolderTotalsReply {
            totals: vec![None; paths.len()],
            busy: true,
        })
    }

    /// Runs `query_many` on `folders` and returns the answers with the size of each request sent.
    fn batch(
        reply: fn(&[String]) -> Option<FolderTotalsReply>,
        folders: &[Location],
    ) -> (Vec<TotalsState>, Vec<usize>) {
        SENT.with(|sent| sent.borrow_mut().clear());
        let answers = ServiceTotals { reply }.query_many(folders, &STOP.with(CancelToken::clone));
        let sizes = SENT.with(|sent| sent.borrow().iter().map(Vec::len).collect());
        (answers, sizes)
    }

    fn numbered(count: usize) -> Vec<Location> {
        (0..count)
            .map(|i| Location::new(format!(r"C:\proj\{i}")).unwrap())
            .collect()
    }

    fn ready(files: u32) -> TotalsState {
        TotalsState::Ready(FolderTotals { files, ..totals() })
    }

    #[test]
    fn fifty_short_folders_take_one_request() {
        let (answers, sizes) = batch(recording, &numbered(50));
        assert_eq!(sizes, [50]);
        assert_eq!(answers, (0..50).map(ready).collect::<Vec<_>>());
    }

    #[test]
    fn hundred_fifty_folders_take_three_requests_in_order() {
        let (answers, sizes) = batch(recording, &numbered(150));
        assert_eq!(
            sizes,
            [64, 64, 22],
            "the pipe takes at most 64 paths per request"
        );
        assert_eq!(answers, (0..150).map(ready).collect::<Vec<_>>());
    }

    #[test]
    fn deep_folders_split_on_the_component_limit() {
        let deep: Vec<Location> = (0..64)
            .map(|i| Location::new(format!(r"C:{}\{i}", r"\d".repeat(98))).unwrap())
            .collect();
        let (answers, sizes) = batch(recording, &deep);
        assert_eq!(
            sizes,
            [40, 24],
            "40 x 100 components fit in 4,096; 41 do not"
        );
        assert_eq!(answers, (0..64).map(ready).collect::<Vec<_>>());
    }

    #[test]
    fn overlong_path_is_unavailable_and_never_sent() {
        let long = Location::new(format!(r"C:\{}", "a".repeat(32_766))).unwrap();
        assert_eq!(long.as_str().len(), 32_769);
        let mut folders = numbered(2);
        folders.insert(1, long);
        let (answers, sizes) = batch(recording, &folders);
        assert_eq!(sizes, [2], "the overlong path joins no request");
        assert_eq!(answers, [ready(0), TotalsState::Unavailable, ready(1)]);
    }

    #[test]
    fn failed_request_marks_only_its_own_chunk() {
        let (answers, sizes) = batch(first_chunk_fails, &numbered(70));
        assert_eq!(sizes, [64, 6]);
        assert!(answers[..64].iter().all(|a| *a == TotalsState::Unavailable));
        assert_eq!(answers[64..], (64..70).map(ready).collect::<Vec<_>>());
    }

    #[test]
    fn a_cancel_after_the_first_chunk_sends_no_further_request() {
        let (answers, sizes) = batch(leave_after_reply, &numbered(150));
        assert_eq!(sizes, [64], "no chunk is sent after the cancel");
        assert_eq!(answers[..64], (0..64).map(ready).collect::<Vec<_>>());
        assert!(answers[64..].iter().all(|a| *a == TotalsState::Unavailable));
    }

    #[test]
    fn busy_and_non_local_folders_keep_their_places() {
        let mut folders = numbered(2);
        folders.insert(1, Location::new("ftp://host/pub").unwrap());
        let (answers, _) = batch(all_busy, &folders);
        assert_eq!(
            answers,
            [
                TotalsState::Busy,
                TotalsState::Unavailable,
                TotalsState::Busy
            ]
        );
        assert!(
            batch(recording, &[]).1.is_empty(),
            "no folders send no request"
        );
    }

    #[test]
    fn count_line_splits_ready_answers_into_complete_and_incomplete() {
        let incomplete = FolderTotals {
            complete: false,
            ..totals()
        };
        let answers = [
            TotalsState::Ready(totals()),
            TotalsState::Ready(totals()),
            TotalsState::Ready(incomplete),
            TotalsState::Busy,
            TotalsState::Unavailable,
        ];
        assert_eq!(
            count_line(&answers, 2),
            "folder totals: folders=5 requests=2 complete=2 busy=1 unavailable=1 incomplete=1"
        );
    }
}
