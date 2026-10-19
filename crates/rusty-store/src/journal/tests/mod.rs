use rusqlite::{Connection, params};

use super::*;

mod crash;
mod create;
mod finish;
mod held;
mod mark;
mod prune;
mod settings;

fn journal() -> JobJournal {
    JobJournal::new(Db::open_in_memory().unwrap()).unwrap()
}

fn conn(journal: &JobJournal) -> &Connection {
    journal.db.connection()
}

fn job_state(journal: &JobJournal, job: JobId) -> String {
    conn(journal)
        .query_row("SELECT state FROM job WHERE id = ?1", [job.0], |row| {
            row.get(0)
        })
        .unwrap()
}

fn step_rows(journal: &JobJournal, job: JobId) -> Vec<(u32, String, Option<Vec<u8>>)> {
    let mut stmt = conn(journal)
        .prepare("SELECT seq, state, detail FROM job_step WHERE job_id = ?1 ORDER BY seq")
        .unwrap();
    stmt.query_map([job.0], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}

fn count(journal: &JobJournal, sql: &str) -> i64 {
    conn(journal)
        .query_row(sql, params![], |row| row.get(0))
        .unwrap()
}

fn pending(seq: u32) -> JournalStep {
    JournalStep {
        seq,
        state: StepState::Pending,
        detail: None,
    }
}
