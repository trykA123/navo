//! Discover portable benchmark targets and expose speed-check commands.

pub mod base;
pub mod config;

pub use base::base_cmd;

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::ExitCode,
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

/// Placeholder for instruction verdicts, returning an unavailable status.
pub fn verdict_cmd(_args: &[String]) -> ExitCode {
    eprintln!("not built yet");
    ExitCode::from(2)
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
pub(super) mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicUsize, Ordering},
    };

    pub(super) struct TestTree(pub PathBuf);

    impl TestTree {
        /// Create an owned temporary workspace, failing on filesystem errors.
        pub(super) fn new() -> std::io::Result<Self> {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(std::io::Error::other)?
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "navo-speed-{}-{time}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }

        /// Add a bench source and its crate manifest, failing on filesystem errors.
        fn bench(&self, package: &str, manifest: &str) -> std::io::Result<()> {
            let path = self.0.join("crates").join(package);
            fs::create_dir_all(path.join("benches"))?;
            fs::write(path.join("benches/listing.rs"), "// fixture")?;
            fs::write(path.join("Cargo.toml"), manifest)
        }
    }

    impl Drop for TestTree {
        /// Remove only this test's owned temporary workspace.
        fn drop(&mut self) {
            if let Err(error) = fs::remove_dir_all(&self.0) {
                eprintln!("temporary workspace cleanup failed: {error}");
            }
        }
    }

    #[test]
    fn targets_are_listed_from_temp_tree() -> Result<(), Box<dyn std::error::Error>> {
        let tree = TestTree::new()?;
        tree.bench(
            "navo-index",
            "[[bench]]\nname = 'listing'\nrequired-features = ['bench', 'test-hooks']",
        )?;
        tree.bench("navo-fs", "[[bench]]\nname = 'listing'")?;
        fs::create_dir(tree.0.join("crates/navo-fs/benches/nested"))?;
        fs::write(tree.0.join("crates/navo-fs/benches/nested/ignored.rs"), "")?;
        assert_eq!(
            discover_targets(&tree.0)?,
            ["navo-fs listing -", "navo-index listing bench,test-hooks"]
        );
        Ok(())
    }

    #[test]
    fn navo_ui_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let tree = TestTree::new()?;
        for package in ["navo-ui", "navo-app", "navo-ui-gallery"] {
            tree.bench(package, "[[bench]]\nname = 'listing'")?;
        }
        let error = discover_targets(&tree.0)
            .err()
            .ok_or("GPUI bench accepted")?;
        assert!(error.contains("GPUI"), "{error}");
        Ok(())
    }

    #[test]
    fn bench_file_without_entry_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let tree = TestTree::new()?;
        tree.bench("navo-fs", "[[bench]]\nname = 'other'")?;
        let error = discover_targets(&tree.0)
            .err()
            .ok_or("undeclared bench accepted")?;
        assert!(
            error.contains("listing") && error.contains("[[bench]]"),
            "{error}"
        );
        Ok(())
    }
}
