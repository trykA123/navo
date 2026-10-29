use std::sync::{Mutex, PoisonError, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rusty_store::{DbError, Frecent};

use crate::files::{SharedDb, lock};

const INTERVAL: Duration = Duration::from_secs(3);
const MAX_PENDING: usize = 10_000;

type Visit = (String, bool, i64);
type Ranking = Result<Vec<Frecent>, DbError>;

enum Message {
    Visit(Visit),
    Rank(usize, i64, mpsc::SyncSender<Ranking>),
    Clear(mpsc::SyncSender<Result<(), DbError>>),
    Stop,
}

pub(crate) struct Visits {
    sender: mpsc::SyncSender<Message>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Visits {
    pub(super) fn start(db: SharedDb) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel(256);
        let worker = std::thread::Builder::new()
            .name("navo-visits".into())
            .spawn(move || run(&db, &receiver))?;
        Ok(Self {
            sender,
            worker: Mutex::new(Some(worker)),
        })
    }

    pub(super) fn record(&self, location: &str, is_folder: bool, now: i64) {
        self.send(Message::Visit((location.to_owned(), is_folder, now)));
    }

    pub(super) fn rank(&self, limit: usize, now: i64) -> Vec<Frecent> {
        let (sender, receiver) = mpsc::sync_channel(1);
        if !self.send(Message::Rank(limit, now, sender)) {
            return Vec::new();
        }
        receiver
            .recv()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "visits worker stopped before ranking");
                Ok(Vec::new())
            })
            .inspect_err(|error| super::report("loading visits", error))
            .unwrap_or_default()
    }

    pub(super) fn clear(&self) {
        let (sender, receiver) = mpsc::sync_channel(1);
        if self.send(Message::Clear(sender)) {
            match receiver.recv() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => super::report("clearing", &error),
                Err(error) => tracing::warn!(%error, "visits worker stopped before clearing"),
            }
        }
    }

    fn send(&self, message: Message) -> bool {
        self.sender
            .send(message)
            .inspect_err(|error| {
                tracing::warn!(%error, "visits worker is unavailable");
            })
            .is_ok()
    }

    pub(crate) fn stop(&self) {
        let worker = self
            .worker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(worker) = worker {
            self.send(Message::Stop);
            if worker.join().is_err() {
                tracing::warn!("visits worker failed at exit");
            }
        }
    }
}

impl Drop for Visits {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Default)]
struct Batch {
    pending: Vec<Visit>,
    deadline: Option<Instant>,
}

impl Batch {
    fn record(&mut self, visit: Visit, now: Instant) {
        self.deadline.get_or_insert(now + INTERVAL);
        self.pending.push(visit);
    }

    fn flush(&mut self, db: &SharedDb) -> Result<(), DbError> {
        lock(db).record_visits(&self.pending)?;
        self.pending.clear();
        self.deadline = None;
        Ok(())
    }

    fn wait_for_capacity(&mut self, db: &SharedDb) -> Result<(), DbError> {
        if self.pending.len() < MAX_PENDING {
            return Ok(());
        }
        if let Some(deadline) = self.deadline {
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        }
        self.flush(db)
    }

    fn flush_due(&mut self, db: &SharedDb, now: Instant) {
        if self.deadline.is_some_and(|deadline| now >= deadline)
            && let Err(error) = self.flush(db)
        {
            super::report("saving visits", &error);
            self.deadline = Some(now + INTERVAL);
        }
    }
}

fn run(db: &SharedDb, receiver: &mpsc::Receiver<Message>) {
    let mut batch = Batch::default();
    loop {
        batch.flush_due(db, Instant::now());
        if let Err(error) = batch.wait_for_capacity(db) {
            super::report("saving a full visit batch", &error);
            return;
        }
        let message = match batch.deadline {
            Some(deadline) => {
                let wait = deadline.saturating_duration_since(Instant::now());
                receiver.recv_timeout(wait)
            }
            None => receiver
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected),
        };
        match message {
            Ok(Message::Visit(visit)) => batch.record(visit, Instant::now()),
            Ok(Message::Rank(limit, now, reply)) => {
                let ranked = batch.flush(db).and_then(|()| lock(db).frecent(limit, now));
                let _ = reply.send(ranked);
            }
            Ok(Message::Clear(reply)) => {
                let cleared = lock(db).clear_search_history();
                if cleared.is_ok() {
                    batch = Batch::default();
                }
                let _ = reply.send(cleared);
            }
            Ok(Message::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                if let Err(error) = batch.flush(db) {
                    super::report("saving visits at exit", &error);
                }
                return;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rusty_store::Db;

    use super::*;

    fn shared() -> SharedDb {
        Arc::new(Mutex::new(
            rusty_store::JobJournal::new(Db::open_in_memory().unwrap()).unwrap(),
        ))
    }

    fn history(db: &SharedDb) -> i64 {
        lock(db)
            .connection()
            .query_row("SELECT count(*) FROM history", [], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn ten_visits_within_three_seconds_commit_once() {
        let db = shared();
        let mut batch = Batch::default();
        let start = Instant::now();
        for n in 0..10 {
            let now = start + Duration::from_millis(n * 200);
            batch.record((format!("/{n}"), true, 1000), now);
            batch.flush_due(&db, now);
        }
        assert_eq!(history(&db), 0);

        batch.flush_due(&db, start + INTERVAL);
        assert_eq!(history(&db), 10);
        batch.flush_due(&db, start + INTERVAL * 2);
        assert_eq!(history(&db), 10);
    }

    #[test]
    fn pending_visits_persist_at_clean_exit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("visits.db");
        let db = crate::jobs::open(&path).unwrap();
        let visits = Visits::start(db.clone()).unwrap();
        visits.record("/pending", true, 1000);
        visits.stop();
        drop(db);
        let reopened = Db::open(&path, false).unwrap().0;
        assert_eq!(reopened.frecent(1, 2000).unwrap()[0].location, "/pending");
    }

    #[test]
    fn ranking_sees_a_visit_from_one_second_ago() {
        let db = shared();
        let visits = Visits::start(db).unwrap();
        visits.record("/pending", true, 1000);
        let ranked = visits.rank(1, 2000);
        assert_eq!(ranked[0].location, "/pending");
        assert_eq!(ranked[0].weight, 4.0);
    }

    #[test]
    fn clearing_pending_visits_does_not_restore_them_at_exit() {
        let db = shared();
        let visits = Visits::start(db.clone()).unwrap();
        visits.record("/pending", true, 1000);
        visits.clear();
        visits.stop();
        assert_eq!(history(&db), 0);
    }

    #[test]
    fn a_failed_batch_keeps_pending_visits_for_retry() {
        let db = shared();
        lock(&db).connection().execute_batch("CREATE TEMP TRIGGER refuse BEFORE INSERT ON history BEGIN SELECT RAISE(ABORT, 'refused'); END;").unwrap();
        let mut batch = Batch::default();
        batch.record(("/pending".into(), true, 1000), Instant::now());
        assert!(batch.flush(&db).is_err());
        assert_eq!(history(&db), 0);
        lock(&db)
            .connection()
            .execute_batch("DROP TRIGGER refuse")
            .unwrap();
        batch.flush(&db).unwrap();
        assert_eq!(history(&db), 1);
    }
}
