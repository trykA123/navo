use std::fmt::Write;

use super::{Incremental, Index, Listing, Memory, Report, Search};

const MB: f64 = 1_000_000.0;

fn megabytes(bytes: Option<u64>) -> String {
    bytes.map_or_else(
        || "n/a".to_owned(),
        |bytes| format!("{:.1} MB", bytes as f64 / MB),
    )
}

fn memory(memory: Memory) -> String {
    format!(
        "{} private, {} resident",
        megabytes(memory.private_bytes),
        megabytes(memory.resident_bytes)
    )
}

pub(in super::super) fn markdown(report: &Report) -> String {
    let mut out = format!(
        "# Navo heavy benchmark\n\nNavo {} ({}), {}, {} CPUs. Fixture: {} files in {} folders under `{}`, \
         generated in {:.1} s (not a Navo number).\n\n",
        report.version,
        report.commit,
        report.os,
        report.cpus,
        report.generation.files,
        report.generation.folders,
        report.root,
        report.generation.ms / 1000.0,
    );
    index(&mut out, &report.index);
    search(&mut out, report.search_quiet_wait_ms, &report.search);
    listing(&mut out, &report.listing);
    incremental(&mut out, &report.incremental);
    out
}

fn index(out: &mut String, index: &Index) {
    let _ = write!(
        out,
        "## Index\n\nPath: {}.\n\n| Measure | Value |\n| --- | --- |\n\
         | Entries | {} |\n| Wall time | {:.1} s |\n| Entries per second | {:.0} |\n\
         | Memory before | {} |\n| Peak while indexing | {} |\n| After indexing | {} |\n\
         | Process peak working set | {} |\n| Index size on disk | {} |\n| Index save time | {:.0} ms |\n\n",
        index.path,
        index.entries,
        index.ms / 1000.0,
        index.entries_per_s,
        memory(index.before),
        memory(index.peak),
        memory(index.after),
        megabytes(index.process_peak_working_set_bytes),
        megabytes(Some(index.snapshot_bytes)),
        index.snapshot_save_ms,
    );
}

fn search(out: &mut String, quiet_wait_ms: f64, rows: &[Search]) {
    out.push_str(
        "## Ctrl+K search\n\n| Query | Text | Results | Lock waits | Cold | p50 | p95 | p99 | Max |\n\
         | --- | --- | --- | --- | --- | --- | --- | --- | --- |\n",
    );
    for row in rows {
        let _ = writeln!(
            out,
            "| {} | `{}` | {} | {} | {:.1} ms | {:.1} ms | {:.1} ms | {:.1} ms | {:.1} ms |",
            row.label,
            row.query,
            row.results,
            row.lock_waits,
            row.cold_ms,
            row.p50_ms,
            row.p95_ms,
            row.p99_ms,
            row.max_ms
        );
    }
    let runs = rows.first().map_or(0, |row| row.warm_runs);
    let _ = writeln!(
        out,
        "\nWarm figures come from {runs} runs per query. The index was quiet after {quiet_wait_ms:.0} ms; a lock wait is a query run that found the index table write-locked and waited for it.\n"
    );
}

fn listing(out: &mut String, rows: &[Listing]) {
    out.push_str("## Folder listing\n\n| Folder | Entries | List | Sorts | Filter keystrokes |\n| --- | --- | --- | --- | --- |\n");
    for row in rows {
        let sorts: Vec<_> = row
            .sort_ms
            .iter()
            .map(|(by, ms)| format!("{by} {ms:.0} ms"))
            .collect();
        let keys: Vec<_> = row
            .filter_keystroke_ms
            .iter()
            .map(|ms| format!("{ms:.0}"))
            .collect();
        let _ = writeln!(
            out,
            "| {} | {} | {:.0} ms | {} | `{}`: {} ms → {} matches |",
            row.folder,
            row.entries,
            row.list_ms,
            sorts.join(", "),
            row.filter,
            keys.join(" / "),
            row.filter_matches
        );
    }
    out.push('\n');
}

fn incremental(out: &mut String, report: &Incremental) {
    out.push_str("## Incremental update\n\n");
    if !report.measured {
        let _ = writeln!(out, "not measured: {}", report.note);
        return;
    }
    let reflected = report.reflected_after_ms.map_or_else(
        || "not reflected before the deadline".to_owned(),
        |ms| format!("reflected {:.1} s after the last change", ms / 1000.0),
    );
    let _ = writeln!(
        out,
        "{} created, {} renamed, {} deleted in {:.1} s; {reflected} ({}).",
        report.created,
        report.renamed,
        report.deleted,
        report.operations_ms / 1000.0,
        report.note
    );
    for error in &report.errors {
        let _ = writeln!(out, "- index error: {error}");
    }
}
