//! Test target discovery and the speed-verdict command boundary.

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

/// Write minimal valid verdict fixtures into an owned temporary tree.
fn write_verdict_inputs(tree: &TestTree) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir(tree.0.join("summaries"))?;
    fs::write(
        tree.0.join("summaries/summary.json"),
        r#"{
        "package_dir":"crates/navo-fs", "module_path":"listing::sort",
        "profiles":[{"tool":"Callgrind","data":{"total":{"metrics":{
            "Ir":{"values":{"new":1000,"old":1000}}
        }}}}]
    }"#,
    )?;
    fs::write(
        tree.0.join("head.toml"),
        "[[bench]]\nid='navo-fs/listing::sort'\nguarded=true\n",
    )?;
    fs::write(tree.0.join("targets"), "navo-fs listing -\n")?;
    fs::write(tree.0.join("out"), "previous report\n")?;
    Ok(())
}

/// Create every verdict input in an owned tree and return the CLI arguments.
fn verdict_inputs(tree: &TestTree) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    write_verdict_inputs(tree)?;
    Ok([
        "--summaries",
        "summaries",
        "--config",
        "head.toml",
        "--base-config",
        "none",
        "--retired",
        "retired.toml",
        "--targets",
        "targets",
        "--base",
        "base-sha",
        "--label",
        "v0.5.0",
        "--mode",
        "guard",
        "--limit",
        "5",
        "--noise-limit",
        "0.5",
        "--out",
        "out",
    ]
    .as_chunks::<2>()
    .0
    .iter()
    .flat_map(|pair| {
        let value = if matches!(
            pair[0],
            "--summaries" | "--config" | "--retired" | "--targets" | "--out"
        ) {
            tree.0.join(pair[1]).to_string_lossy().into_owned()
        } else {
            pair[1].into()
        };
        [pair[0].into(), value]
    })
    .collect())
}

#[test]
fn verdict_command_appends_markdown_and_returns_success() -> Result<(), Box<dyn std::error::Error>>
{
    let tree = TestTree::new()?;
    assert_eq!(verdict_cmd(&verdict_inputs(&tree)?), ExitCode::SUCCESS);
    let output = fs::read_to_string(tree.0.join("out"))?;
    assert!(output.starts_with("previous report\n"));
    assert!(
        output.contains("v0.5.0") && output.contains("| ID | base `Ir` | head `Ir` | diff % |")
    );
    Ok(())
}

#[test]
fn verdict_command_fails_on_missing_empty_and_unknown_head_manifest()
-> Result<(), Box<dyn std::error::Error>> {
    let tree = TestTree::new()?;
    let args = verdict_inputs(&tree)?;
    fs::remove_file(tree.0.join("head.toml"))?;
    assert_eq!(verdict_cmd(&args), ExitCode::FAILURE);
    for source in ["", "unexpected=true"] {
        fs::write(tree.0.join("head.toml"), source)?;
        assert_eq!(verdict_cmd(&args), ExitCode::FAILURE);
    }
    Ok(())
}

#[test]
fn verdict_command_rejects_bad_flags_and_thresholds() -> Result<(), Box<dyn std::error::Error>> {
    let tree = TestTree::new()?;
    let args = verdict_inputs(&tree)?;
    for (flag, value) in [
        ("--mode", "other"),
        ("--limit", "NaN"),
        ("--noise-limit", "-1"),
    ] {
        let mut invalid = args.clone();
        let index = invalid
            .iter()
            .position(|arg| arg == flag)
            .ok_or("missing flag")?;
        invalid[index + 1] = value.into();
        assert_ne!(verdict_cmd(&invalid), ExitCode::SUCCESS);
    }
    assert_ne!(verdict_cmd(&[]), ExitCode::SUCCESS);
    let mut duplicate = args.clone();
    duplicate.extend(["--base".into(), "other".into()]);
    assert_ne!(verdict_cmd(&duplicate), ExitCode::SUCCESS);
    Ok(())
}
