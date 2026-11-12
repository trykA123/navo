// The hidden pin probe for the installer VM checks: holds an existing root and reports whether
// the hold still matches after a wait. Split from `tester.rs` to keep that file under 400 lines.

use std::ffi::OsString;
use std::process::ExitCode;

#[cfg(windows)]
use std::ffi::OsStr;

#[cfg(windows)]
use serde_json::{Value, json};

#[cfg(windows)]
use crate::probe::chain::resolve_chain;
#[cfg(windows)]
use crate::probe::pin::Pin;
#[cfg(windows)]
use crate::syntax::parse_candidate;

/// Runs the pin probe and holds the root for the requested number of seconds.
#[cfg(windows)]
pub(super) fn pin_probe(path: Option<&OsString>, seconds: Option<&OsString>) -> ExitCode {
    let seconds = seconds.and_then(|value| value.to_str());
    let seconds = seconds.and_then(|value| value.parse().ok());
    let (Some(path), Some(seconds)) = (path, seconds) else {
        println!("usage: navo-setup --tester-probe pin <path> <seconds>");
        return ExitCode::from(2);
    };
    let report = pin_report(path, seconds)
        .unwrap_or_else(|error| json!({"path": path.to_string_lossy(), "error": error}));
    let Ok(report) = serde_json::to_string_pretty(&report) else {
        println!("navo-setup: cannot print the pin report");
        return ExitCode::from(2);
    };
    println!("{report}");
    ExitCode::SUCCESS
}

/// Holds an existing root, sleeps, then reports whether the holds still match.
#[cfg(windows)]
fn pin_report(path: &OsStr, seconds: u64) -> Result<Value, String> {
    let candidate = parse_candidate(path).map_err(|problem| problem.message().to_owned())?;
    let chain = resolve_chain(&candidate).map_err(|error| error.to_string())?;
    let last = chain
        .existing
        .last()
        .ok_or_else(|| "root has no identity".to_owned())?;
    let pin = Pin::existing(candidate.path(), last.id).map_err(|error| error.to_string())?;
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    Ok(json!({
        "path": path.to_string_lossy(),
        "identity": format!("{:032x}", last.id.id),
        "unchanged": pin.unchanged(),
    }))
}

/// The pin probe is Windows-only; Linux reports it unsupported and exits 2.
#[cfg(not(windows))]
pub(super) fn pin_probe(_path: Option<&OsString>, _seconds: Option<&OsString>) -> ExitCode {
    println!("navo-setup: pin probe is unsupported on this platform");
    ExitCode::from(2)
}
