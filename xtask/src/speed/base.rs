//! Select the strict-ancestor product release and validate SC3 recovery bases.

use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Output, Stdio},
    time::{Duration, Instant},
};

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseInfo {
    tag_name: String,
    is_draft: bool,
}

#[derive(Debug)]
pub struct Release {
    pub tag: String,
    pub is_draft: bool,
    pub commit: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Base {
    pub sha: String,
    pub label: String,
}

/// Choose a release or recovery commit; fail when no valid strict ancestor exists.
pub fn choose_base(
    releases: &[Release],
    head: &str,
    recovery: Option<&str>,
    is_ancestor: impl Fn(&str, &str) -> bool,
) -> Result<Base, String> {
    let selected = releases
        .iter()
        .filter_map(|release| product_version(&release.tag).map(|version| (version, release)))
        .filter(|(_, release)| !release.is_draft)
        .filter(|(_, release)| release.commit != head && is_ancestor(&release.commit, head))
        .max_by_key(|(version, _)| *version)
        .map(|(_, release)| release)
        .ok_or("no eligible strict-ancestor product release")?;
    let Some(recovery) = recovery else {
        return Ok(Base {
            sha: selected.commit.clone(),
            label: selected.tag.clone(),
        });
    };
    if recovery == head
        || recovery == selected.commit
        || !is_ancestor(recovery, head)
        || !is_ancestor(&selected.commit, recovery)
    {
        return Err("recovery must be strictly between the selected release and HEAD".into());
    }
    Ok(Base {
        sha: recovery.into(),
        label: recovery.into(),
    })
}

/// Read three decimal version components; other tag formats return no version.
fn product_version(tag: &str) -> Option<[(usize, &str); 3]> {
    let parts: [&str; 3] = tag
        .strip_prefix('v')?
        .split('.')
        .collect::<Vec<_>>()
        .try_into()
        .ok()?;
    if parts
        .iter()
        .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    Some(parts.map(|part| {
        let digits = part.trim_start_matches('0');
        (digits.len(), digits)
    }))
}

/// Print the chosen SHA and label; invalid inputs or git failures exit with status 2.
pub fn base_cmd(args: &[String]) -> ExitCode {
    match select_base(args) {
        Ok(base) => {
            println!("BASE_SHA={}", base.sha);
            println!("BASE_LABEL={}", base.label);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("speed-base: {error}");
            ExitCode::from(2)
        }
    }
}

struct Options {
    releases: PathBuf,
    head: String,
    config: PathBuf,
}

/// Parse each required flag once, rejecting missing values and unknown arguments.
fn parse_options(args: &[String]) -> Result<Options, String> {
    let usage = "usage: cargo xtask speed-base --releases <json> --head <sha> --config <file>";
    let (mut releases, mut head, mut config) = (None, None, None);
    let mut args = args.iter();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--releases" if releases.is_none() => {
                releases = Some(PathBuf::from(args.next().ok_or(usage)?));
            }
            "--head" if head.is_none() => head = Some(args.next().ok_or(usage)?.clone()),
            "--config" if config.is_none() => {
                config = Some(PathBuf::from(args.next().ok_or(usage)?));
            }
            _ => return Err(usage.into()),
        }
    }
    Ok(Options {
        releases: releases.ok_or(usage)?,
        head: head.ok_or(usage)?,
        config: config.ok_or(usage)?,
    })
}

/// Resolve input refs and call the pure selector, preserving every git failure.
fn select_base(args: &[String]) -> Result<Base, String> {
    let options = parse_options(args)?;
    let config = super::config::load_config(&options.config)?;
    let recovery = match (&config.recovery_base, &config.recovery_reason) {
        (None, None) => None,
        (Some(base), Some(reason)) if !base.trim().is_empty() && !reason.trim().is_empty() => {
            Some(resolve_commit(base)?)
        }
        _ => {
            return Err(
                "recovery_base and a non-empty recovery_reason are required together".into(),
            );
        }
    };
    let releases = load_releases(&options.releases)?;
    let head = resolve_commit(&options.head)?;
    let failure = RefCell::new(None);
    let base = choose_base(
        &releases,
        &head,
        recovery.as_deref(),
        |base, head| match git_ancestor(base, head) {
            Ok(ancestor) => ancestor,
            Err(error) => {
                *failure.borrow_mut() = Some(error);
                false
            }
        },
    );
    match failure.into_inner() {
        Some(error) => Err(error),
        None => base,
    }
}

/// Read release JSON and resolve eligible product tags, failing on input or git errors.
fn load_releases(path: &Path) -> Result<Vec<Release>, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("reading {}: {error}", path.display()))?;
    let entries: Vec<ReleaseInfo> = serde_json::from_str(&source)
        .map_err(|error| format!("parsing {}: {error}", path.display()))?;
    let mut releases = Vec::new();
    for entry in entries {
        if product_version(&entry.tag_name).is_some() && !entry.is_draft {
            releases.push(Release {
                commit: resolve_commit(&entry.tag_name)?,
                tag: entry.tag_name,
                is_draft: false,
            });
        }
    }
    Ok(releases)
}

/// Resolve a ref to its commit SHA, failing on missing refs or empty git output.
fn resolve_commit(reference: &str) -> Result<String, String> {
    let output = git_output(&["rev-list", "-n", "1", "--end-of-options", reference])?;
    if !output.status.success() {
        return Err(format!(
            "resolving {reference}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let sha = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    if sha.trim().is_empty() {
        return Err(format!("no commit found for {reference}"));
    }
    Ok(sha.trim().into())
}

/// Check git ancestry; exit 1 means unrelated and all other failures are errors.
fn git_ancestor(base: &str, head: &str) -> Result<bool, String> {
    let output = git_output(&["merge-base", "--is-ancestor", base, head])?;
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => Err(format!(
            "checking ancestry {base}..{head}: {}",
            String::from_utf8_lossy(&output.stderr)
        )),
    }
}

/// Run local git with a 30-second deadline, reaping the child before returning.
fn git_output(args: &[&str]) -> Result<Output, String> {
    let mut child = Command::new("git")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("starting git {args:?}: {error}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().map_err(|error| error.to_string()),
            Ok(None) if started.elapsed() < Duration::from_secs(30) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            result => {
                let killed = child.kill();
                let reaped = child.wait();
                return Err(format!(
                    "git {args:?} timed out or could not be polled: {result:?}; kill={killed:?}; wait={reaped:?}"
                ));
            }
        }
    }
}

#[cfg(test)]
#[path = "base_tests.rs"]
mod tests;
