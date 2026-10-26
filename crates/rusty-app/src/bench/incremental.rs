use std::fs::{self, File};
use std::io;
use std::path::Path;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use rusty_index::host::Host;
use rusty_index::table::NameTable;

use super::fixture::{INCREMENTAL, context, victim_name};
use super::report::{self, Incremental};

const DEADLINE: Duration = Duration::from_secs(300);
const CREATED: &str = "fresh-";
const RENAMED: &str = "moved-";
const VICTIM: &str = "victim-";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Changes {
    pub(super) created: usize,
    pub(super) renamed: usize,
    pub(super) deleted: usize,
}

impl Changes {
    pub(super) fn of(operations: usize) -> Self {
        let renamed = operations * 3 / 10;
        Self {
            created: operations - 2 * renamed,
            renamed,
            deleted: renamed,
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Seen {
    pub(super) created: usize,
    pub(super) renamed: usize,
    pub(super) victims: usize,
}

pub(super) fn apply(fixture: &Path, changes: Changes) -> io::Result<()> {
    let folder = fixture.join(INCREMENTAL);
    for index in 0..changes.created {
        let path = folder.join(format!("{CREATED}{index:05}.c"));
        File::options()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| context("creating", &path, error))?;
    }
    for index in 0..changes.renamed {
        let from = folder.join(victim_name(index));
        let to = folder.join(format!("{RENAMED}{index:05}.c"));
        fs::rename(&from, &to).map_err(|error| context("renaming", &from, error))?;
    }
    for index in changes.renamed..changes.renamed + changes.deleted {
        let path = folder.join(victim_name(index));
        fs::remove_file(&path).map_err(|error| context("deleting", &path, error))?;
    }
    Ok(())
}

pub(super) fn seen(table: &NameTable) -> Seen {
    table.iter().fold(Seen::default(), |mut seen, (_, name)| {
        seen.created += usize::from(name.starts_with(CREATED));
        seen.renamed += usize::from(name.starts_with(RENAMED));
        seen.victims += usize::from(name.starts_with(VICTIM));
        seen
    })
}

fn wait_until_seen(table: &RwLock<NameTable>, want: Seen) -> Option<Duration> {
    let started = Instant::now();
    while started.elapsed() < DEADLINE {
        if table.try_read().is_ok_and(|table| seen(&table) == want) {
            return Some(started.elapsed());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    None
}

pub(super) fn not_measured(changes: Changes, note: &str) -> Incremental {
    Incremental {
        measured: false,
        note: note.to_owned(),
        created: changes.created,
        renamed: changes.renamed,
        deleted: changes.deleted,
        operations_ms: 0.0,
        reflected_after_ms: None,
        errors: Vec::new(),
    }
}

pub(super) fn run(fixture: &Path, host: &Host, changes: Changes) -> Result<Incremental, String> {
    let started = Instant::now();
    apply(fixture, changes).map_err(|error| format!("changing files: {error}"))?;
    let operations_ms = report::ms(started.elapsed());
    let want = Seen {
        created: changes.created,
        renamed: changes.renamed,
        victims: 0,
    };
    let reflected = wait_until_seen(&host.table(), want);
    let note = match reflected {
        Some(_) => {
            "measured through the recursive folder watcher that keeps the walk index current"
        }
        None => "the index did not show every change within 300 s",
    };
    Ok(Incremental {
        measured: true,
        note: note.to_owned(),
        operations_ms,
        reflected_after_ms: reflected.map(report::ms),
        errors: host
            .try_status()
            .map(|status| status.errors)
            .unwrap_or_default(),
        ..not_measured(changes, "")
    })
}
