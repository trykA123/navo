//! Only `src/probe/spawn.rs` starts a process outside test helpers, and it touches no files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SPAWN_FILE: &str = "src/probe/spawn.rs";
/// Helpers compiled only under `cfg(test)` that run `attrib`, `cmd` or `rustc`.
const TEST_HELPERS: [&str; 4] = [
    "src/fixture.rs",
    "src/guard/test_support.rs",
    "src/guard/windows_tests.rs",
    "src/privacy/api_tests.rs",
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

/// Process-start spellings, split so this file does not match itself.
fn starters() -> Vec<String> {
    [
        ["Command", "::new"],
        ["process::", "Command"],
        ["CreateProcess", ""],
        ["ShellExecute", ""],
        ["WinExec", ""],
        ["libc::", "fork"],
        ["libc::", "exec"],
        ["posix_", "spawn"],
    ]
    .iter()
    .map(|parts| parts.concat())
    .collect()
}

#[test]
fn processes_start_only_in_the_probe_spawn_file() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    sources(&crate_root.join("src"), &mut files).unwrap();
    sources(&crate_root.join("examples"), &mut files).unwrap();
    assert!(files.len() > 20, "the source walk must see the crate");
    let needles = starters();
    let mut offenders: Vec<String> = files
        .iter()
        .filter(|path| {
            let text = fs::read_to_string(path).unwrap();
            needles.iter().any(|needle| text.contains(needle.as_str()))
        })
        .map(|path| {
            let relative = path.strip_prefix(crate_root).unwrap();
            relative.to_string_lossy().replace('\\', "/")
        })
        .filter(|path| path != SPAWN_FILE && !TEST_HELPERS.contains(&path.as_str()))
        .collect();
    offenders.sort();
    assert!(offenders.is_empty(), "process start in {offenders:?}");
}

#[test]
fn the_spawn_file_starts_suspended_and_touches_no_files() {
    let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let text = fs::read_to_string(crate_root.join(SPAWN_FILE)).unwrap();
    assert!(text.contains("CREATE_SUSPENDED"));
    for banned in ["fs::", "File", "OpenOptions", "unsafe"] {
        assert!(!text.contains(banned), "{banned} in the spawn file");
    }
}
