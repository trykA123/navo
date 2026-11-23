//! Dependency-graph guard for `cargo xtask seams` (WS1): checks the declared
//! direct dependencies of workspace members from `cargo metadata`.

use std::path::Path;
use std::process::Command;

use serde_json::Value;

/// Edges that break rule 2 but are allowed for now; the P plan removes this one.
pub const ALLOWED: [(&str, &str); 1] = [("navo-ui", "navo-automotive")];

const SIDEWISE_RULE: &str = "no sidewise crate may depend on navo-app or navo-ui";
const ALLOWLIST_RULE: &str = "edge not in the allowlist";

/// Run `cargo metadata --no-deps` in `root` and check its graph against `ALLOWED`.
///
/// Returns the violation lines; fails when cargo cannot run or its output is not metadata.
pub fn check(root: &Path) -> Result<Vec<String>, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("cannot run cargo metadata: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cargo metadata failed: {}", stderr.trim()));
    }
    let json = String::from_utf8(output.stdout)
        .map_err(|error| format!("cargo metadata output is not UTF-8: {error}"))?;
    violations(&json, &ALLOWED)
}

/// Return one sorted `crate -> dependency: rule` line per forbidden direct edge in `json`.
///
/// Rule 1: no `sidewise-*` crate depends on `navo-app` or `navo-ui`. Rule 2:
/// `navo-ui -> navo-automotive` only while it is in `allowed`. Each edge is
/// reported once, whatever its kind (normal, dev, build). Fails on malformed JSON.
pub fn violations(json: &str, allowed: &[(&str, &str)]) -> Result<Vec<String>, String> {
    let metadata: Value =
        serde_json::from_str(json).map_err(|error| format!("bad metadata JSON: {error}"))?;
    let packages = metadata["packages"]
        .as_array()
        .ok_or("metadata has no packages list")?;
    let mut lines = Vec::new();
    for package in packages {
        let from = package["name"].as_str().ok_or("package without a name")?;
        let deps = package["dependencies"]
            .as_array()
            .ok_or_else(|| format!("{from}: no dependencies list"))?;
        for dep in deps {
            let to = dep["name"]
                .as_str()
                .ok_or_else(|| format!("{from}: dependency without a name"))?;
            if let Some(rule) = broken_rule(from, to, allowed) {
                lines.push(format!("{from} -> {to}: {rule}"));
            }
        }
    }
    lines.sort();
    lines.dedup();
    Ok(lines)
}

/// The rule the edge `from -> to` breaks, if any.
fn broken_rule(from: &str, to: &str, allowed: &[(&str, &str)]) -> Option<&'static str> {
    if from.starts_with("sidewise-") && (to == "navo-app" || to == "navo-ui") {
        return Some(SIDEWISE_RULE);
    }
    let gated = from == "navo-ui" && to == "navo-automotive";
    (gated && !allowed.contains(&(from, to))).then_some(ALLOWLIST_RULE)
}
