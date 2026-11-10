//! Discover portable benchmark targets and expose speed-check commands.

pub mod base;
pub mod config;
pub mod summary;
pub mod verdict;

pub use base::base_cmd;

use std::{
    collections::BTreeMap,
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    time::{Duration, Instant},
};

use serde::Deserialize;

/// List configured bench targets; invalid arguments or inputs fail the command.
pub fn targets_cmd(args: &[String]) -> ExitCode {
    let result = target_options(args).and_then(|(path, discover)| {
        let config = config::load_config(&path)?;
        if !discover && config.benches.is_empty() {
            return Err(format!("{} has no [[bench]] entries", path.display()));
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("cannot determine workspace root")?;
        discover_targets(root)
    });
    match result {
        Ok(lines) => {
            for line in lines {
                println!("{line}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("speed-targets: {error}");
            ExitCode::from(2)
        }
    }
}

/// Parse a required config path and optional discovery flag, rejecting other arguments.
fn target_options(args: &[String]) -> Result<(PathBuf, bool), String> {
    let mut path = None;
    let mut discover = false;
    let mut args = args.iter();
    let usage = "usage: cargo xtask speed-targets --config <file> [--discover]";
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--config" if path.is_none() => {
                path = Some(PathBuf::from(args.next().ok_or(usage)?));
            }
            "--discover" if !discover => discover = true,
            _ => return Err(usage.into()),
        }
    }
    Ok((path.ok_or(usage)?, discover))
}

/// Print and append an instruction verdict; input errors or regressions fail.
pub fn verdict_cmd(args: &[String]) -> ExitCode {
    let options = match verdict_options(args) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("speed-verdict: {error}");
            return ExitCode::from(2);
        }
    };
    let mut report = build_verdict(&options).unwrap_or_else(|error| verdict::Report {
        lines: vec![format!("speed-verdict: {error}")],
        failed: true,
    });
    let bootstrap = if options["--base-config"] == "none" {
        "; no base manifest"
    } else {
        ""
    };
    report.lines.insert(
        0,
        format!(
            "Base: {} ({}){bootstrap}",
            options["--label"], options["--base"]
        ),
    );
    let output = format!("{}\n", report.lines.join("\n"));
    print!("{output}");
    if let Err(error) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(options["--out"])
        .and_then(|mut file| file.write_all(output.as_bytes()))
    {
        eprintln!("appending {}: {error}", options["--out"]);
        return ExitCode::FAILURE;
    }
    if report.failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Require every verdict flag exactly once, rejecting missing values and unknown flags.
fn verdict_options(args: &[String]) -> Result<BTreeMap<&str, &str>, String> {
    let flags = [
        "--summaries",
        "--config",
        "--base-config",
        "--retired",
        "--targets",
        "--base",
        "--label",
        "--mode",
        "--limit",
        "--noise-limit",
        "--out",
    ];
    let mut options = BTreeMap::new();
    let (pairs, remainder) = args.as_chunks::<2>();
    for pair in pairs {
        if !flags.contains(&pair[0].as_str())
            || options.insert(pair[0].as_str(), pair[1].as_str()).is_some()
        {
            return Err(format!("unknown or duplicate flag: {}", pair[0]));
        }
    }
    if !remainder.is_empty() || flags.iter().any(|flag| !options.contains_key(flag)) {
        return Err(format!("required flags: {}", flags.join(" ")));
    }
    Ok(options)
}

/// Load verdict inputs and run the pure rules; invalid files or thresholds return errors.
fn build_verdict(options: &BTreeMap<&str, &str>) -> Result<verdict::Report, String> {
    let mode = match options["--mode"] {
        "guard" => verdict::Mode::Guard,
        "noise" => verdict::Mode::Noise,
        other => return Err(format!("invalid verdict mode: {other}")),
    };
    let limit = percent(options["--limit"])?;
    let noise_limit = percent(options["--noise-limit"])?;
    let head = config::load_config(Path::new(options["--config"]))?;
    let base = match options["--base-config"] {
        "none" => None,
        path => Some(config::load_config(Path::new(path))?),
    };
    let retired = config::load_retired(Path::new(options["--retired"]))?;
    let targets_path = options["--targets"];
    let targets: Vec<_> = fs::read_to_string(targets_path)
        .map_err(|error| format!("reading {targets_path}: {error}"))?
        .lines()
        .map(String::from)
        .collect();
    let measured = summary::collect(Path::new(options["--summaries"]))?;
    Ok(verdict::verdict(
        &measured,
        &head,
        base.as_ref(),
        &retired,
        options["--base"],
        &targets,
        mode,
        limit,
        noise_limit,
        resolve_ref,
    ))
}

/// Parse a non-negative percentage in tenths into permille; reject invalid or finer limits.
fn percent(value: &str) -> Result<u64, String> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let fraction = fraction.trim_end_matches('0');
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 1
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!("invalid percentage: {value}"));
    }
    whole
        .parse::<u64>()
        .ok()
        .and_then(|whole| whole.checked_mul(10))
        .and_then(|whole| whole.checked_add(fraction.parse::<u64>().unwrap_or(0)))
        .ok_or_else(|| format!("invalid percentage: {value}"))
}

/// Resolve a commit ref with a 30-second deadline; unavailable refs do not match the base.
fn resolve_ref(reference: &str) -> Option<String> {
    let mut child = Command::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let output = child.wait_with_output().ok()?;
                return status
                    .success()
                    .then(|| String::from_utf8_lossy(&output.stdout).trim().into());
            }
            Ok(None) if start.elapsed() < Duration::from_secs(30) => {
                std::thread::sleep(Duration::from_millis(10));
            }
            result => {
                let killed = child.kill();
                let reaped = child.wait();
                eprintln!(
                    "resolving {reference}: git timeout or poll failure: {result:?}; kill={killed:?}; wait={reaped:?}"
                );
                return None;
            }
        }
    }
}

/// Discover bench output lines beneath a workspace, failing on invalid targets.
fn discover_targets(root: &Path) -> Result<Vec<String>, String> {
    let mut lines = Vec::new();
    for entry in read_directory(&root.join("crates"))? {
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_dir()
        {
            lines.extend(crate_targets(&path)?);
        }
    }
    lines.sort();
    Ok(lines)
}

#[derive(Deserialize)]
struct Manifest {
    #[serde(default)]
    bench: Vec<BenchTarget>,
}

#[derive(Deserialize)]
struct BenchTarget {
    name: String,
    #[serde(default, rename = "required-features")]
    features: Vec<String>,
}

/// Read one directory with path context, failing on unreadable entries.
fn read_directory(path: &Path) -> Result<Vec<fs::DirEntry>, String> {
    fs::read_dir(path)
        .and_then(|entries| entries.collect())
        .map_err(|error| format!("reading {}: {error}", path.display()))
}

/// Match a crate's immediate bench files to Cargo entries, rejecting GPUI crates.
fn crate_targets(path: &Path) -> Result<Vec<String>, String> {
    let directory = path.join("benches");
    match fs::metadata(&directory) {
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("reading {}: {error}", directory.display())),
        Ok(_) => {}
    }
    let mut sources = Vec::new();
    for entry in read_directory(&directory)? {
        let source = entry.path();
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
            && source
                .extension()
                .is_some_and(|extension| extension == "rs")
        {
            sources.push(source);
        }
    }
    if sources.is_empty() {
        return Ok(Vec::new());
    }
    let package = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid crate name: {}", path.display()))?;
    if matches!(package, "navo-ui" | "navo-app" | "navo-ui-gallery") {
        return Err(format!("GPUI crate {package} cannot contain speed benches"));
    }
    let manifest_path = path.join("Cargo.toml");
    let source = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("reading {}: {error}", manifest_path.display()))?;
    let manifest: Manifest = toml::from_str(&source)
        .map_err(|error| format!("parsing {}: {error}", manifest_path.display()))?;
    sources
        .iter()
        .map(|source| target_line(package, source, &manifest))
        .collect()
}

/// Format one declared target with comma-separated features, failing if undeclared.
fn target_line(package: &str, source: &Path, manifest: &Manifest) -> Result<String, String> {
    let name = source
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid bench name: {}", source.display()))?;
    let target = manifest
        .bench
        .iter()
        .find(|target| target.name == name)
        .ok_or_else(|| format!("{} has no [[bench]] entry for {name}", source.display()))?;
    let features = if target.features.is_empty() {
        "-".into()
    } else {
        target.features.join(",")
    };
    Ok(format!("{package} {name} {features}"))
}

#[cfg(test)]
#[path = "mod_tests.rs"]
pub(super) mod tests;
