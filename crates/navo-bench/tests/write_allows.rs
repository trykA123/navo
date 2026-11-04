//! Only the guard and a few named files may switch off the write-method lint.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Files outside `src/guard/` that may allow the lint, each with a reason in place.
const ALLOWED: [&str; 7] = [
    "src/source/platform.rs",
    "src/source/policy.rs",
    "src/source/windows_tests.rs",
    "src/fixture.rs",
    "src/manifest/mutate.rs",
    "src/privacy/api_tests.rs",
    "src/probe/spawn.rs",
];

fn sources(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            sources(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

#[test]
fn write_lint_is_switched_off_only_in_the_guard_and_named_files() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let needle = ["clippy::", "disallowed_methods"].concat();
    let mut files = Vec::new();
    sources(&crate_root.join("src"), &mut files).unwrap();
    assert!(files.len() > 20, "the source walk must see the crate");
    let mut offenders: Vec<String> = files
        .iter()
        .filter(|path| fs::read_to_string(path).unwrap().contains(&needle))
        .map(|path| {
            let relative = path.strip_prefix(crate_root).unwrap();
            relative.to_string_lossy().replace('\\', "/")
        })
        .filter(|relative| !relative.starts_with("src/guard/"))
        .filter(|relative| !ALLOWED.contains(&relative.as_str()))
        .collect();
    offenders.sort();
    assert!(offenders.is_empty(), "lint switched off in {offenders:?}");
}
