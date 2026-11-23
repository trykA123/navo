//! Portable audit (NV9b): a portable start (folders, store ensure, instance lock, crash socket)
//! writes nothing to the temp, app-data, runtime or home folders. The test re-runs itself in a
//! child process with those variables pointed at empty folders, so no other test sees them.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Set in the child process to the portable root it starts from.
const CHILD_ROOT: &str = "NAVO_NV9B_PORTABLE_ROOT";
const TEST_NAME: &str =
    "crash::portable_env_tests::a_portable_start_writes_nothing_outside_its_folder";
/// Every variable the startup paths could read for a folder outside the portable copy.
const OUTSIDE_VARS: [&str; 11] = [
    "TEMP",
    "TMP",
    "TMPDIR",
    "APPDATA",
    "LOCALAPPDATA",
    "USERPROFILE",
    "HOME",
    "XDG_RUNTIME_DIR",
    "XDG_CONFIG_HOME",
    "XDG_DATA_HOME",
    "XDG_CACHE_HOME",
];

/// Lists every entry under `folder`, depth first; fails when a folder cannot be read.
fn entries(folder: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if path.is_dir() {
            found.extend(entries(&path)?);
        }
        found.push(path);
    }
    Ok(found)
}

/// The portable start, run in the child: the same calls the app makes for a portable copy.
fn start_portable(root: &Path) {
    let exe = root.join("navo.exe");
    let dirs = navo_store::StoreDirs::detect_at(&exe).unwrap();
    assert_eq!(dirs.data, root);
    dirs.ensure().unwrap();
    dirs.remove_orphan_temps().unwrap();
    let run = navo_store::StoreDirs::portable_run_at(&exe).unwrap();
    let acquired = crate::instance::acquire(&dirs.data, Some(&run)).unwrap();
    assert!(matches!(
        acquired,
        Some(crate::instance::Acquired::First(_))
    ));
    let socket =
        super::socket::path(Some(&run), &std::env::temp_dir(), std::process::id()).unwrap();
    assert!(socket.starts_with(root), "{}", socket.display());
}

#[test]
fn a_portable_start_writes_nothing_outside_its_folder() {
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        start_portable(Path::new(&root));
        return;
    }
    let scratch = tempfile::tempdir().unwrap();
    let root = scratch.path().join("Portable");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("navo.toml"), b"").unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap());
    child.args(["--exact", TEST_NAME, "--nocapture", "--test-threads=1"]);
    child.env(CHILD_ROOT, &root);
    let outside = scratch.path().join("outside");
    for name in OUTSIDE_VARS {
        let folder = outside.join(name);
        std::fs::create_dir_all(&folder).unwrap();
        child.env(name, &folder);
    }
    let output = child.output().unwrap();
    let log = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "child start failed:\n{log}");
    assert!(
        log.contains("1 passed"),
        "child did not run the test:\n{log}"
    );
    let written: Vec<PathBuf> = entries(&outside)
        .unwrap()
        .into_iter()
        .filter(|path| !OUTSIDE_VARS.iter().any(|name| path == &outside.join(name)))
        .collect();
    assert!(
        written.is_empty(),
        "portable start wrote outside its folder: {written:?}"
    );
}
