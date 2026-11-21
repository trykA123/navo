//! Rewrites the hash cache file from the live rows: temp file, sync, then rename.
//! Also decides when the log is worth rewriting at open, and trims rows past the cap.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::Path;

use super::file::{Row, with_suffix};
use super::record::{Digest, MAGIC, encode};

/// Replaces the file at `path` with `MAGIC` plus every row in write order.
/// Fails on any I/O error; the temp file is removed then and `path` is left as it was.
pub(super) fn rewrite(path: &Path, rows: &HashMap<Digest, Row>) -> io::Result<()> {
    let temp = with_suffix(path, ".tmp");
    let result = write_rows(&temp, rows).and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Writes `MAGIC` and the rows sorted by `seq` to a new file at `temp`, then syncs it.
fn write_rows(temp: &Path, rows: &HashMap<Digest, Row>) -> io::Result<()> {
    let mut ordered: Vec<_> = rows.iter().collect();
    ordered.sort_unstable_by_key(|(_, row)| row.seq);
    let mut out = BufWriter::new(File::create(temp)?);
    out.write_all(&MAGIC)?;
    for (key, row) in ordered {
        out.write_all(&encode(key, &row.hash))?;
    }
    out.into_inner()
        .map_err(io::IntoInnerError::into_error)?
        .sync_all()
}

/// Row limits: how many rows survive an open-time rewrite, and when `put` stops adding keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Rows kept at open; older rows are dropped from memory and the file.
    pub cap_rows: usize,
    /// Rows at which `put` refuses new keys until the next open.
    pub ceiling_rows: usize,
}

impl Limits {
    /// 500,000 rows (a 36 MB file, about 40 MB of memory); new keys stop at 625,000.
    pub const DEFAULT: Self = Self {
        cap_rows: 500_000,
        ceiling_rows: 625_000,
    };
}

/// Spare records a log may hold beyond twice its live rows before a rewrite pays off.
const SLACK_RECORDS: usize = 4096;

/// True when a healthy log should still be rewritten at open: it holds more than
/// `2 × live + 4096` records (`records` counts bad ones too), or `live` passes the cap.
pub(super) fn needs_rewrite(records: usize, live: usize, limits: Limits) -> bool {
    records > live.saturating_mul(2).saturating_add(SLACK_RECORDS) || live > limits.cap_rows
}

/// Keeps only the newest `cap` rows by `seq` (the last write wins) and drops the rest.
pub(super) fn keep_newest(rows: &mut HashMap<Digest, Row>, cap: usize) {
    let excess = rows.len().saturating_sub(cap);
    if excess == 0 {
        return;
    }
    let mut seqs: Vec<u64> = rows.values().map(|row| row.seq).collect();
    // Selection, not a full sort: seqs are unique, so this is the newest seq that goes.
    let (_, &mut cutoff, _) = seqs.select_nth_unstable(excess - 1);
    rows.retain(|_, row| row.seq > cutoff);
}
