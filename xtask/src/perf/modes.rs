//! `perf-summary` modes: SC11's CLI, SC10's fail-closed checks, `fresh`, `vm`, `calibrate`
//! and `aggregate`.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Deserialize;

mod calibrate;
use calibrate::{Baseline, Verdict, aggregate as aggregate_lines, calibration_line, judge};

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

/// SC10: `done.json` holds one exit per launch, and `perf.ps1` launches Navo six times.
const DONE_EXITS: usize = 6;

/// Usage of the `fresh` mode, as SC11's table.
const FRESH_USAGE: &str =
    "usage: cargo xtask perf-summary <reports-dir> <summary-file> --mode fresh --commit <sha>";

/// Usage of the `vm` mode, as SC11's table.
const VM_USAGE: &str = concat!(
    "usage: cargo xtask perf-summary <reports-dir> <summary-file> --mode vm",
    " --commit <sha> --done <file> --baseline <file> --fingerprint <text>",
);

/// Usage of the `calibrate` mode, as SC11's table.
const CALIBRATE_USAGE: &str = concat!(
    "usage: cargo xtask perf-summary <reports-dir> <summary-file> --mode calibrate",
    " --commit <sha> --done <file> --fingerprint <text>",
);

/// Usage of the `aggregate` mode, which reads lines instead of a run folder.
const AGGREGATE_USAGE: &str = concat!(
    "usage: cargo xtask perf-summary --mode aggregate <lines-file>",
    " --measured <YYYY-MM-DD> --reason <text>",
);

/// Usage when no known mode is at hand, e.g. a mode name this build does not have.
const USAGE: &str = concat!(
    "usage: cargo xtask perf-summary <reports-dir> <summary-file> --mode <fresh|vm|calibrate>",
    " or --mode aggregate <lines-file> --measured <date> --reason <text>",
);

/// A mode of `perf-summary`, one row of SC11's table.
#[derive(Debug)]
enum Mode {
    /// Check the six reports against the fixture, then append today's table and guard.
    Fresh {
        root: PathBuf,
        output: PathBuf,
        commit: String,
    },
    /// SC11's verdict of one VM run against the calibrated baseline.
    Vm { run: RunArgs, baseline: PathBuf },
    /// One calibration line for `aggregate`, from a run judged against nothing.
    Calibrate { run: RunArgs },
    /// Ten calibration lines folded into the baseline TOML, printed to stdout.
    Aggregate {
        lines: PathBuf,
        measured: String,
        reason: String,
    },
}

/// The arguments `vm` and `calibrate` share: one run folder and the VM it ran on.
#[derive(Debug)]
struct RunArgs {
    root: PathBuf,
    output: PathBuf,
    commit: String,
    done: PathBuf,
    fingerprint: String,
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

/// SC11's flags, each optional, each given at most once, in any order around the
/// positionals. `given` keeps the names seen, so a mode can reject another mode's flag.
#[derive(Default)]
struct Flags<'a> {
    mode: Option<&'a str>,
    commit: Option<&'a str>,
    done: Option<&'a str>,
    baseline: Option<&'a str>,
    fingerprint: Option<&'a str>,
    measured: Option<&'a str>,
    reason: Option<&'a str>,
    given: Vec<&'a str>,
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
        Mode::Vm {
            run: args,
            baseline,
        } => match vm(&args, &baseline) {
            Ok(code) => code,
            Err(error) => super::finish(Err(error)),
        },
        Mode::Calibrate { run: args } => super::finish(calibrate(&args)),
        Mode::Aggregate {
            lines,
            measured,
            reason,
        } => super::finish(aggregate(&lines, &measured, &reason)),
    }
}

/// Parse SC11's arguments: positionals in either place, flags in any order. A missing,
/// repeated or unknown flag, a missing value, an unknown mode or a malformed `--commit`
/// is usage.
fn parse(args: &[String]) -> Result<Mode, String> {
    let (positionals, flags) = scan(args)?;
    let name = flags.mode.ok_or_else(|| usage(None, "--mode is missing"))?;
    only(name, &flags.given)?;
    match name {
        "fresh" => {
            let (root, output) = run_paths(&positionals, name)?;
            Ok(Mode::Fresh {
                root,
                output,
                commit: sha(flags.commit, name)?,
            })
        }
        "vm" => {
            let run = run_args(&positionals, &flags, name)?;
            Ok(Mode::Vm {
                run,
                baseline: PathBuf::from(require(flags.baseline, "--baseline", name)?),
            })
        }
        "calibrate" => Ok(Mode::Calibrate {
            run: run_args(&positionals, &flags, name)?,
        }),
        "aggregate" => {
            let [lines] = positionals.as_slice() else {
                return Err(usage(Some(name), "expected <lines-file>"));
            };
            Ok(Mode::Aggregate {
                lines: PathBuf::from(*lines),
                measured: require(flags.measured, "--measured", name)?.to_owned(),
                reason: require(flags.reason, "--reason", name)?.to_owned(),
            })
        }
        other => Err(usage(Some(other), &format!("unknown mode {other}"))),
    }
}

/// Split the arguments into positionals and flag values.
fn scan(args: &[String]) -> Result<(Vec<&str>, Flags<'_>), String> {
    let mut positionals = Vec::new();
    let mut flags = Flags::default();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let arg = arg.as_str();
        let mode = flags.mode;
        let slot = match arg {
            "--mode" => &mut flags.mode,
            "--commit" => &mut flags.commit,
            "--done" => &mut flags.done,
            "--baseline" => &mut flags.baseline,
            "--fingerprint" => &mut flags.fingerprint,
            "--measured" => &mut flags.measured,
            "--reason" => &mut flags.reason,
            _ if arg.starts_with("--") => {
                return Err(usage(mode, &format!("unknown flag {arg}")));
            }
            _ => {
                positionals.push(arg);
                continue;
            }
        };
        if slot.is_some() {
            return Err(usage(mode, &format!("{arg} given twice")));
        }
        *slot = Some(value(&mut iter, arg, mode)?);
        flags.given.push(arg);
    }
    Ok((positionals, flags))
}

/// The flags SC11's table gives a mode. A flag of another mode is usage, never ignored.
fn only(name: &str, given: &[&str]) -> Result<(), String> {
    let allowed: &[&str] = match name {
        "fresh" => &["--mode", "--commit"],
        "vm" => &[
            "--mode",
            "--commit",
            "--done",
            "--baseline",
            "--fingerprint",
        ],
        "calibrate" => &["--mode", "--commit", "--done", "--fingerprint"],
        "aggregate" => &["--mode", "--measured", "--reason"],
        _ => return Ok(()),
    };
    match given.iter().find(|flag| !allowed.contains(flag)) {
        Some(flag) => Err(usage(Some(name), &format!("{flag} is not a {name} flag"))),
        None => Ok(()),
    }
}

/// The `<reports-dir> <summary-file>` pair of the modes that read one run folder.
fn run_paths(positionals: &[&str], name: &str) -> Result<(PathBuf, PathBuf), String> {
    let [root, output] = positionals else {
        return Err(usage(Some(name), "expected <reports-dir> <summary-file>"));
    };
    Ok((PathBuf::from(*root), PathBuf::from(*output)))
}

/// The arguments `vm` and `calibrate` share, SC11's columns of one run.
fn run_args(positionals: &[&str], flags: &Flags<'_>, name: &str) -> Result<RunArgs, String> {
    let (root, output) = run_paths(positionals, name)?;
    Ok(RunArgs {
        root,
        output,
        commit: sha(flags.commit, name)?,
        done: PathBuf::from(require(flags.done, "--done", name)?),
        fingerprint: require(flags.fingerprint, "--fingerprint", name)?.to_owned(),
    })
}

/// A required flag's value or a usage error naming it.
fn require<'a>(value: Option<&'a str>, flag: &str, name: &str) -> Result<&'a str, String> {
    value.ok_or_else(|| usage(Some(name), &format!("{flag} is missing")))
}

/// SC11's `--commit`: required, and a full 40-character lowercase sha.
fn sha(value: Option<&str>, name: &str) -> Result<String, String> {
    let value = require(value, "--commit", name)?;
    if is_sha(value) {
        return Ok(value.to_owned());
    }
    Err(usage(
        Some(name),
        "--commit is not 40-character lowercase hex",
    ))
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
        Some("vm") => VM_USAGE,
        Some("calibrate") => CALIBRATE_USAGE,
        Some("aggregate") => AGGREGATE_USAGE,
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

/// SC10: `done.json` is written last and is exactly `{"exits":[0,0,0,0,0,0],
/// "finished":"<iso>"}` — two keys, six integers all zero, and `finished` the string
/// `perf.ps1` stamped. Anything else means the run did not happen as B4 defines it.
pub(super) fn check_done(json: &str) -> Result<(), String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|error| format!("not done.json: {error}"))?;
    let object = value.as_object().ok_or("done.json is not an object")?;
    if object.len() != 2 {
        return Err("done.json is not the two keys exits and finished".to_owned());
    }
    if !object
        .get("finished")
        .is_some_and(serde_json::Value::is_string)
    {
        return Err("done.json has no finished string".to_owned());
    }
    let exits = object
        .get("exits")
        .and_then(|exits| exits.as_array())
        .ok_or("done.json has no exits array")?;
    if exits.len() != DONE_EXITS {
        return Err(format!(
            "done.json has {} exits, not {DONE_EXITS}",
            exits.len()
        ));
    }
    match exits.iter().find(|exit| exit.as_u64() != Some(0)) {
        Some(exit) => Err(format!("a launch exited {exit}")),
        None => Ok(()),
    }
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

/// `vm`: SC10's checks, then SC11's verdict. Every verdict prints and appends its line;
/// the exit code is 0, 1 or SC11's 4 for "not calibrated".
fn vm(run: &RunArgs, baseline: &Path) -> Result<ExitCode, String> {
    let done = read_done(&run.done)?;
    check_done(&done)?;
    let numbers = load_run(&run.root, &run.commit)?;
    let baseline = read_baseline(baseline)?;
    let (label, detail, code) = match judge(&numbers, baseline.as_ref(), &run.fingerprint) {
        Verdict::Pass(detail) => ("pass", detail, ExitCode::SUCCESS),
        Verdict::Fail(detail) => ("FAIL", detail, ExitCode::FAILURE),
        Verdict::NotCalibrated(detail) => ("not calibrated", detail, ExitCode::from(4)),
    };
    emit(&run.output, &format!("perf: {label}: {detail}"))?;
    Ok(code)
}

/// `calibrate`: SC10's checks, then the line SC11's `aggregate` needs ten of.
fn calibrate(run: &RunArgs) -> Result<(), String> {
    let done = read_done(&run.done)?;
    check_done(&done)?;
    let numbers = load_run(&run.root, &run.commit)?;
    emit(
        &run.output,
        &calibration_line(&run.commit, &run.fingerprint, &numbers),
    )
}

/// `aggregate`: ten calibration lines to one baseline, on stdout only (SC11).
fn aggregate(lines: &Path, measured: &str, reason: &str) -> Result<(), String> {
    let text =
        fs::read_to_string(lines).map_err(|error| format!("{}: {error}", lines.display()))?;
    print!("{}", aggregate_lines(&text, measured, reason)?);
    Ok(())
}

/// SC10's launch log, naming the file B4 writes last.
fn read_done(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))
}

/// SC11: a missing baseline means "not calibrated"; one that does not parse is a failure.
fn read_baseline(path: &Path) -> Result<Option<Baseline>, String> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str::<Baseline>(&text)
            .map(Some)
            .map_err(|error| format!("{}: {error}", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!("{}: {error}", path.display())),
    }
}

/// Print SC11's line and append the same line to the summary file.
fn emit(output: &Path, line: &str) -> Result<(), String> {
    println!("{line}");
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(output)
        .map_err(|error| format!("{}: {error}", output.display()))?;
    writeln!(file, "{line}").map_err(|error| format!("{}: {error}", output.display()))
}
