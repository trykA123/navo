//! `perf-summary` modes: SC11's CLI, SC10's fail-closed report checks and `fresh`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;

#[cfg(test)]
mod tests;

/// Every report of a fixture run must carry these pane counts: `A` holds 5,000 files and
/// `B` holds 200 folders, exactly as `perf/fixture.rs` generates them.
pub(super) const EXPECTED_PANES: [u64; 2] = [5000, 200];

/// SC10's shortest acceptable `commit`: `NAVO_COMMIT` is a 12-character short sha, so
/// anything under 8 characters is a truncated or stale reading rather than this build.
const MIN_COMMIT_CHARS: usize = 8;

/// The six report names of one run, in SC10's order.
const REPORT_NAMES: [&str; 6] = ["first", "warm-1", "warm-2", "warm-3", "warm-4", "warm-5"];

/// Usage of the `fresh` mode, as SC11's table.
const FRESH_USAGE: &str =
    "usage: cargo xtask perf-summary <reports-dir> <summary-file> --mode fresh --commit <sha>";

/// Usage when no known mode is at hand, e.g. a mode name this build does not have.
const USAGE: &str =
    "usage: cargo xtask perf-summary <reports-dir> <summary-file> --mode <fresh> --commit <sha>";

/// A mode of `perf-summary`. B3c adds the VM, calibration and aggregate variants.
#[derive(Debug)]
enum Mode {
    /// Check the six reports against the fixture, then append today's table and guard.
    Fresh {
        root: PathBuf,
        output: PathBuf,
        commit: String,
    },
}

/// One report's SC10-checked numbers.
pub(super) struct Fresh {
    pub(super) private_bytes: u64,
    pub(super) first_usable_ms: f64,
}

/// One run's numbers, as SC11 defines them for the six reports of a run.
pub(super) struct Run {
    pub(super) private_bytes: u64,
    pub(super) warm_ms: f64,
}

/// The four metrics fields SC10 reads; the rest of a report belongs to `report.rs`.
#[derive(Deserialize)]
struct Reading {
    commit: String,
    first_usable_ms: f64,
    idle_private_bytes: Option<u64>,
    pane_entries: Option<Vec<u64>>,
}

/// Run the mode named in `args`, mapping errors to SC11's exit codes: 2 for usage, 1 for
/// a failed check.
pub(super) fn run(args: &[String]) -> ExitCode {
    let mode = match parse(args) {
        Ok(mode) => mode,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    match mode {
        Mode::Fresh {
            root,
            output,
            commit,
        } => super::finish(fresh(&root, &output, &commit)),
    }
}

/// Parse SC11's arguments: two positionals, then flags in any order. A missing, repeated
/// or unknown flag, a missing value, an unknown mode or a malformed `--commit` is usage.
fn parse(args: &[String]) -> Result<Mode, String> {
    let mut positionals = Vec::new();
    let mut mode: Option<&str> = None;
    let mut commit: Option<&str> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let arg = arg.as_str();
        if arg == "--mode" {
            if mode.is_some() {
                return Err(usage(mode, "--mode given twice"));
            }
            mode = Some(value(&mut iter, "--mode", mode)?);
        } else if arg == "--commit" {
            if commit.is_some() {
                return Err(usage(mode, "--commit given twice"));
            }
            commit = Some(value(&mut iter, "--commit", mode)?);
        } else if arg.starts_with("--") {
            return Err(usage(mode, &format!("unknown flag {arg}")));
        } else if positionals.len() < 2 && mode.is_none() && commit.is_none() {
            positionals.push(arg);
        } else {
            return Err(usage(
                mode,
                "only <reports-dir> and <summary-file> are positional",
            ));
        }
    }
    if positionals.len() != 2 {
        return Err(usage(mode, "expected <reports-dir> <summary-file>"));
    }
    let Some(name) = mode else {
        return Err(usage(None, "--mode is missing"));
    };
    if name != "fresh" {
        return Err(usage(Some(name), &format!("unknown mode {name}")));
    }
    let Some(commit) = commit else {
        return Err(usage(Some(name), "--commit is missing"));
    };
    if !is_sha(commit) {
        return Err(usage(
            Some(name),
            "--commit is not 40-character lowercase hex",
        ));
    }
    Ok(Mode::Fresh {
        root: PathBuf::from(positionals[0]),
        output: PathBuf::from(positionals[1]),
        commit: commit.to_owned(),
    })
}

/// The value that follows a flag, or a usage error naming the flag when the line ends.
fn value<'a>(
    iter: &mut impl Iterator<Item = &'a String>,
    flag: &str,
    mode: Option<&str>,
) -> Result<&'a str, String> {
    match iter.next() {
        Some(next) => Ok(next.as_str()),
        None => Err(usage(mode, &format!("{flag} has no value"))),
    }
}

/// The reason and the usage line of the mode that was asked for, as one stderr message.
fn usage(mode: Option<&str>, reason: &str) -> String {
    let line = match mode {
        Some("fresh") => FRESH_USAGE,
        _ => USAGE,
    };
    format!("{line} — {reason}")
}

/// SC11: a `--commit` is a full 40-character lowercase sha.
fn is_sha(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Check one metrics report against SC10 and return the two numbers it gates.
pub(super) fn check_report(json: &str, sha: &str) -> Result<Fresh, String> {
    let reading: Reading =
        serde_json::from_str(json).map_err(|error| format!("not a metrics report: {error}"))?;
    if reading.commit.len() < MIN_COMMIT_CHARS {
        return Err(format!(
            "commit '{}' is shorter than {MIN_COMMIT_CHARS} characters",
            reading.commit
        ));
    }
    if !sha.starts_with(reading.commit.as_str()) {
        return Err(format!(
            "commit {} is not a prefix of {sha}",
            reading.commit
        ));
    }
    let Some(private_bytes) = reading.idle_private_bytes.filter(|bytes| *bytes > 0) else {
        return Err("idle_private_bytes absent or zero".to_owned());
    };
    let Some(panes) = reading.pane_entries else {
        return Err("pane_entries absent".to_owned());
    };
    if panes != EXPECTED_PANES {
        return Err(format!(
            "pane_entries {panes:?} are not the fixture's {EXPECTED_PANES:?}"
        ));
    }
    Ok(Fresh {
        private_bytes,
        first_usable_ms: reading.first_usable_ms,
    })
}

/// Read the six reports of one run and fold them into SC11's two numbers.
pub(super) fn load_run(root: &Path, sha: &str) -> Result<Run, String> {
    let mut private_bytes = 0_u64;
    let mut warm: Vec<f64> = Vec::with_capacity(REPORT_NAMES.len() - 1);
    for (index, name) in REPORT_NAMES.iter().enumerate() {
        let path = root.join(format!("{name}.json"));
        let json =
            fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
        let reading =
            check_report(&json, sha).map_err(|error| format!("{}: {error}", path.display()))?;
        private_bytes = private_bytes.max(reading.private_bytes);
        if index > 0 {
            warm.push(reading.first_usable_ms);
        }
    }
    warm.sort_by(f64::total_cmp);
    Ok(Run {
        private_bytes,
        warm_ms: warm[warm.len() / 2],
    })
}

/// `fresh`: SC10's checks first, then today's table and guard (SC11's order).
fn fresh(root: &Path, output: &Path, commit: &str) -> Result<(), String> {
    let run = load_run(root, commit)?;
    println!(
        "perf: fresh ok, private {} bytes, warm {:.1} ms",
        run.private_bytes, run.warm_ms
    );
    super::summarize(root, output)
}
