// Hidden probe dispatch for the installer VM checks; one arm per probe with W7a–W7d.

use std::ffi::OsString;
use std::process::ExitCode;

mod pin;
use pin::pin_probe;

#[cfg(windows)]
use std::ffi::OsStr;
#[cfg(windows)]
use std::io;

#[cfg(windows)]
use serde_json::{Value, json};

#[cfg(windows)]
use crate::mode::{Discovery, Install, Mode, Why, mode as decide};
#[cfg(windows)]
use crate::placement::{Chain, DriveKind, Facts, Link, Protected, Refused, Space, check};
#[cfg(windows)]
use crate::probe::chain::{OrdinalNames, refused, resolve_chain};
#[cfg(windows)]
use crate::probe::discovery::discover;
#[cfg(windows)]
use crate::probe::facts::{elevated, facts as read_facts};
#[cfg(windows)]
use crate::syntax::{FolderProblem, parse_candidate};

/// Dispatches a hidden `--tester-probe <kind>` command; `None` means it is not a probe.
pub fn dispatch(args: &[OsString]) -> Option<ExitCode> {
    let (flag, rest) = args.split_first()?;
    if flag == "--tester-probe" {
        Some(probe(rest))
    } else {
        None
    }
}

/// Runs one probe kind. W7b–W7d add their arms here.
fn probe(args: &[OsString]) -> ExitCode {
    match args.first().and_then(|arg| arg.to_str()) {
        Some("chain") => chain(args.get(1)),
        Some("discover") => discover_probe(),
        Some("facts") => facts_probe(args.get(1)),
        Some("pin") => pin_probe(args.get(1), args.get(2)),
        _ => ExitCode::from(2),
    }
}

/// Runs the chain probe and prints the resolved chain and the placement verdict as JSON.
#[cfg(windows)]
fn chain(path: Option<&OsString>) -> ExitCode {
    let Some(path) = path else {
        println!("usage: navo-setup --tester-probe chain <path>");
        return ExitCode::from(2);
    };
    match serde_json::to_string_pretty(&chain_report(path)) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("navo-setup: cannot print the chain report: {error}");
            ExitCode::from(2)
        }
    }
}

/// Builds the chain report; every API error becomes the `ProbeFailed` verdict.
#[cfg(windows)]
fn chain_report(path: &OsStr) -> Value {
    let text = path.to_string_lossy().into_owned();
    let candidate = match parse_candidate(path) {
        Ok(candidate) => candidate,
        Err(problem) => {
            return json!({
                "path": text,
                "chain": Value::Null,
                "refused": Value::Null,
                "verdict": problem_json(problem),
            });
        }
    };
    let chain = match resolve_chain(&candidate) {
        Ok(chain) => chain,
        Err(error) => return failed_report(text, &error),
    };
    let refused = match refused() {
        Ok(refused) => refused,
        Err(error) => return failed_report(text, &error),
    };
    // W7a proves the chain and refused sets; W7b supplies the real drive facts.
    let facts = Facts {
        drive: DriveKind::Fixed,
        subst: false,
        parent_open: false,
        writable: true,
        free: u64::MAX,
    };
    let space = Space {
        payload: 0,
        compressed: 0,
        extracted: 0,
    };
    let verdict = check(&chain, &refused, &facts, &space, &OrdinalNames);
    json!({
        "path": text,
        "chain": chain_json(&chain),
        "refused": refused_json(&refused),
        "facts": "assumed fixed, writable and unlimited; W7b probes the real facts",
        "verdict": verdict_json(verdict),
    })
}

/// The report for a path whose chain or refused set could not be read.
#[cfg(windows)]
fn failed_report(path: String, error: &io::Error) -> Value {
    json!({
        "path": path,
        "chain": Value::Null,
        "refused": Value::Null,
        "error": error.to_string(),
        "verdict": problem_json(FolderProblem::ProbeFailed),
    })
}

/// One folder problem as JSON, with the debug name and the user-facing line.
#[cfg(windows)]
fn problem_json(problem: FolderProblem) -> Value {
    json!({
        "ok": false,
        "problem": format!("{problem:?}"),
        "message": problem.message(),
    })
}

/// A placement result as JSON.
#[cfg(windows)]
fn verdict_json(result: Result<(), FolderProblem>) -> Value {
    match result {
        Ok(()) => json!({ "ok": true }),
        Err(problem) => problem_json(problem),
    }
}

/// The resolved chain as JSON.
#[cfg(windows)]
fn chain_json(chain: &Chain) -> Value {
    json!({
        "existing": chain.existing.iter().map(link_json).collect::<Vec<_>>(),
        "missing": chain
            .missing
            .iter()
            .map(|name| name.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
    })
}

/// One existing ancestor as JSON.
#[cfg(windows)]
fn link_json(link: &Link) -> Value {
    json!({
        "path": link.path.to_string_lossy().into_owned(),
        "name": link.name.to_string_lossy().into_owned(),
        "volume": link.id.volume,
        "id": format!("{:032x}", link.id.id),
        "reparse": link.reparse,
    })
}

/// The refused sets as JSON.
#[cfg(windows)]
fn refused_json(refused: &Refused) -> Value {
    json!({
        "never_inside": refused.never_inside.iter().map(protected_json).collect::<Vec<_>>(),
        "never_equal": refused.never_equal.iter().map(protected_json).collect::<Vec<_>>(),
        "fresh_only": refused.fresh_only.iter().map(protected_json).collect::<Vec<_>>(),
    })
}

/// One protected folder as JSON.
#[cfg(windows)]
fn protected_json(protected: &Protected) -> Value {
    json!({
        "volume": protected.anchor.volume,
        "id": format!("{:032x}", protected.anchor.id),
        "anchor_case_sensitive": protected.anchor_case_sensitive,
        "tail": protected
            .tail
            .iter()
            .map(|name| name.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
    })
}

/// Runs the facts probe and prints the drive facts and the placement verdict as JSON.
#[cfg(windows)]
fn facts_probe(path: Option<&OsString>) -> ExitCode {
    let Some(path) = path else {
        println!("usage: navo-setup --tester-probe facts <path>");
        return ExitCode::from(2);
    };
    match serde_json::to_string_pretty(&facts_report(path)) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("navo-setup: cannot print the facts report: {error}");
            ExitCode::from(2)
        }
    }
}

/// Builds the facts report; every API error becomes the `ProbeFailed` verdict.
#[cfg(windows)]
fn facts_report(path: &OsStr) -> Value {
    let text = path.to_string_lossy().into_owned();
    let elevated = elevated().unwrap_or(true);
    let candidate = match parse_candidate(path) {
        Ok(candidate) => candidate,
        Err(problem) => {
            return json!({
                "path": text,
                "elevated": elevated,
                "facts": Value::Null,
                "verdict": problem_json(problem),
            });
        }
    };
    let chain = match resolve_chain(&candidate) {
        Ok(chain) => chain,
        Err(error) => return facts_failed(text, elevated, &error),
    };
    let refused = match refused() {
        Ok(refused) => refused,
        Err(error) => return facts_failed(text, elevated, &error),
    };
    let facts = match read_facts(&chain) {
        Ok(facts) => facts,
        Err(error) => return facts_failed(text, elevated, &error),
    };
    let space = Space {
        payload: 0,
        compressed: 0,
        extracted: 0,
    };
    let verdict = check(&chain, &refused, &facts, &space, &OrdinalNames);
    json!({
        "path": text,
        "elevated": elevated,
        "facts": facts_json(&facts),
        "verdict": verdict_json(verdict),
    })
}

/// The report for a path whose chain or facts could not be read.
#[cfg(windows)]
fn facts_failed(path: String, elevated: bool, error: &io::Error) -> Value {
    json!({
        "path": path,
        "elevated": elevated,
        "facts": Value::Null,
        "error": error.to_string(),
        "verdict": problem_json(FolderProblem::ProbeFailed),
    })
}

/// The drive facts as JSON.
#[cfg(windows)]
fn facts_json(facts: &Facts) -> Value {
    json!({
        "drive": format!("{:?}", facts.drive),
        "subst": facts.subst,
        "writable": facts.writable,
        "free": facts.free,
    })
}

/// Runs the discovery probe and prints its facts and the mode decision as JSON.
#[cfg(windows)]
fn discover_probe() -> ExitCode {
    match serde_json::to_string_pretty(&discover_report()) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("navo-setup: cannot print the discovery report: {error}");
            ExitCode::from(2)
        }
    }
}

/// Builds the discovery report; a probe error is printed rather than guessed.
#[cfg(windows)]
fn discover_report() -> Value {
    let discovery = match discover() {
        Ok(discovery) => discovery,
        Err(error) => {
            return json!({
                "discovery": Value::Null,
                "mode": Value::Null,
                "error": error.to_string(),
            });
        }
    };
    let own = match navo_update::parse_version(env!("CARGO_PKG_VERSION")) {
        Ok(version) => version,
        Err(error) => {
            return json!({
                "discovery": discovery_json(&discovery),
                "mode": Value::Null,
                "error": format!("{error:?}"),
            });
        }
    };
    let decided = decide(&discovery, &own);
    json!({
        "discovery": discovery_json(&discovery),
        "mode": mode_json(&decided),
    })
}

/// The discovery facts as JSON.
#[cfg(windows)]
fn discovery_json(discovery: &Discovery) -> Value {
    json!({
        "registry": match &discovery.registry {
            None => Value::Null,
            Some(Ok(install)) => json!({"ok": install_json(install)}),
            Some(Err(why)) => json!({"error": why_json(why)}),
        },
        "legacy": discovery.legacy.as_ref().map_or(Value::Null, install_json),
    })
}

/// One install's facts as JSON.
#[cfg(windows)]
fn install_json(install: &Install) -> Value {
    json!({
        "root": install.root.to_string_lossy().into_owned(),
        "identity": {
            "volume": install.identity.volume,
            "file": format!("{:032x}", u128::from_le_bytes(install.identity.file)),
            "kind": format!("{:?}", install.identity.kind),
        },
        "safe_chain": install.safe_chain,
        "update_exe": install.update_exe,
        "sq_version": match &install.sq_version {
            Ok(sq) => json!({"ok": true, "pack_id": sq.pack_id, "version": sq.version.to_string()}),
            Err(error) => json!({"ok": false, "error": error.to_string()}),
        },
        "marker": match &install.marker {
            None => Value::Null,
            Some(Ok(marker)) => json!({
                "ok": true,
                "pack_id": marker.pack_id,
                "install_id": marker.install_id,
                "created_by": marker.created_by,
            }),
            Some(Err(error)) => json!({"ok": false, "error": error.to_string()}),
        },
    })
}

/// The mode decision as JSON.
#[cfg(windows)]
fn mode_json(mode: &Mode) -> Value {
    match mode {
        Mode::Fresh => json!({"kind": "fresh"}),
        Mode::Upgrade(install) => {
            json!({"kind": "upgrade", "root": install.root.to_string_lossy().into_owned()})
        }
        Mode::Same(install) => {
            json!({"kind": "same", "root": install.root.to_string_lossy().into_owned()})
        }
        Mode::Newer(install) => {
            json!({"kind": "newer", "root": install.root.to_string_lossy().into_owned()})
        }
        Mode::Ambiguous(why) => json!({"kind": "ambiguous", "why": why_json(why)}),
    }
}

/// A discovery refusal as JSON.
#[cfg(windows)]
fn why_json(why: &Why) -> Value {
    match why {
        Why::UnsafeName { root, character } => json!({
            "reason": "unsafe_name",
            "root": root.to_string_lossy().into_owned(),
            "character": character.to_string(),
        }),
        other => json!({"reason": format!("{other:?}")}),
    }
}

/// The chain, discovery and facts probes are Windows-only; Linux reports them unsupported.
#[cfg(not(windows))]
fn chain(_path: Option<&OsString>) -> ExitCode {
    println!("navo-setup: chain probe is unsupported on this platform");
    ExitCode::from(2)
}

/// The facts probe is Windows-only; Linux reports it unsupported and exits 2.
#[cfg(not(windows))]
fn facts_probe(_path: Option<&OsString>) -> ExitCode {
    println!("navo-setup: facts probe is unsupported on this platform");
    ExitCode::from(2)
}

/// The discovery probe is Windows-only; Linux reports it unsupported and exits 2.
#[cfg(not(windows))]
fn discover_probe() -> ExitCode {
    println!("navo-setup: discovery probe is unsupported on this platform");
    ExitCode::from(2)
}
