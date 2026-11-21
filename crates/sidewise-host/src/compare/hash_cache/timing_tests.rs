//! Ignored timing harness for the hash cache: first, cached and recheck compare runs on big trees.
//! Run by hand (and on the VM in plan V1); it prints one `P6 timing:` line.

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use navo_compare::model::HashCache;

use super::file::FileHashes;
use super::policy::RecheckHashes;
use super::reuse_tests::run_once_within;

/// Files changed by one byte on the right side.
const CHANGED: usize = 20;
const KIB: usize = 1024;
const WAIT: Duration = Duration::from_secs(1800);

/// One side's file mix: (big count, big bytes, small count, small bytes).
struct Scale {
    big: usize,
    big_len: usize,
    small: usize,
    small_len: usize,
}

/// Reads `SW_P6_SCALE`: `small` (default) or `large`.
fn scale() -> (Scale, bool) {
    match std::env::var("SW_P6_SCALE").as_deref() {
        Ok("large") => (
            Scale {
                big: 800,
                big_len: 2048 * KIB,
                small: 8000,
                small_len: 16 * KIB,
            },
            true,
        ),
        _ => (
            Scale {
                big: 50,
                big_len: 256 * KIB,
                small: 500,
                small_len: 4 * KIB,
            },
            false,
        ),
    }
}

/// Fills `len` bytes from a xorshift stream seeded by `seed`, so files differ.
fn content(seed: u64, len: usize) -> Vec<u8> {
    let mut state = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut bytes = Vec::with_capacity(len + 8);
    while bytes.len() < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        bytes.extend_from_slice(&state.to_le_bytes());
    }
    bytes.truncate(len);
    bytes
}

/// Sets a file's modified time.
fn set_mtime(path: &Path, time: SystemTime) {
    let file = File::options().write(true).open(path).unwrap();
    file.set_modified(time).unwrap();
}

/// Writes the left tree and a right copy with `CHANGED` big files changed by one byte.
fn build_tree(root: &Path, scale: &Scale) -> usize {
    let (left, right) = (root.join("left"), root.join("right"));
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    let old = SystemTime::now() - Duration::from_secs(3600);
    let mut names = Vec::new();
    for i in 0..scale.big {
        names.push((format!("big{i:05}.bin"), scale.big_len, i));
    }
    for i in 0..scale.small {
        names.push((format!("small{i:05}.bin"), scale.small_len, scale.big + i));
    }
    for (name, len, seed) in &names {
        let mut bytes = content(*seed as u64 + 1, *len);
        File::create(left.join(name))
            .unwrap()
            .write_all(&bytes)
            .unwrap();
        if name.starts_with("big") && *seed < CHANGED {
            bytes[len / 2] ^= 0xFF;
        }
        File::create(right.join(name))
            .unwrap()
            .write_all(&bytes)
            .unwrap();
        set_mtime(&left.join(name), old);
        set_mtime(&right.join(name), old);
    }
    names.len()
}

/// Picks the tree root: `SW_P6_ROOT`, else a directory under the build target.
fn root_dir() -> PathBuf {
    match std::env::var_os("SW_P6_ROOT") {
        Some(root) => PathBuf::from(root),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/p6-timing")
            .join(std::process::id().to_string()),
    }
}

/// Times one compare run in milliseconds.
fn timed(root: &Path, cache: Arc<dyn HashCache>) -> (u128, u64, navo_compare::session::Totals) {
    let started = Instant::now();
    let totals = run_once_within(&root.join("left"), &root.join("right"), cache, WAIT);
    (started.elapsed().as_millis(), totals.different, totals)
}

#[test]
#[ignore = "timing run; start it by hand"]
#[allow(
    clippy::print_stdout,
    reason = "the timing line is the output of this harness"
)]
fn p6_timing_second_run_skips_reading() {
    let (scale, large) = scale();
    let root = root_dir();
    let _ = fs::remove_dir_all(&root);
    let files = build_tree(&root, &scale);
    let store = root.join("cache").join("hashes.v1");

    let first_store = Arc::new(FileHashes::open(&store));
    let (first_ms, _, a) = timed(&root, first_store.clone());
    drop(first_store);
    let second_store = Arc::new(FileHashes::open(&store));
    let (cached_ms, _, b) = timed(&root, second_store.clone());
    let (recheck_ms, different, c) = timed(&root, Arc::new(RecheckHashes(second_store)));

    println!(
        "P6 timing: files={files} first_ms={first_ms} cached_ms={cached_ms} \
         recheck_ms={recheck_ms} different={different}"
    );
    if std::env::var_os("SW_P6_KEEP").is_none() {
        let _ = fs::remove_dir_all(&root);
    }
    assert_eq!(a, b, "cached run totals");
    assert_eq!(a, c, "recheck run totals");
    assert_eq!(different, CHANGED as u64);
    if large {
        assert!(
            cached_ms * 2 <= recheck_ms,
            "cached run is not twice as fast"
        );
    }
}

#[cfg(windows)]
#[test]
#[ignore = "needs the C$ admin share; start it by hand"]
fn p6_network_paths_skip_the_cache() {
    use super::reuse_tests::pane_run;

    // Always the small mix in its own folder: a large tree over SMB outlasts the run wait.
    let scale = Scale {
        big: 50,
        big_len: 256 * KIB,
        small: 500,
        small_len: 4 * KIB,
    };
    let root = root_dir().join("net");
    let _ = fs::remove_dir_all(&root);
    build_tree(&root, &scale);
    let text = root.to_str().unwrap();
    let (drive, rest) = text.split_at(2);
    let share = format!(r"\\localhost\{}${}", &drive[..1], rest);
    let store = Arc::new(FileHashes::memory());
    let (left, right) = (
        Path::new(&share).join("left"),
        Path::new(&share).join("right"),
    );
    pane_run(&left, &right, store.clone(), false);
    pane_run(&left, &right, store.clone(), false);
    let len = store.len();
    if std::env::var_os("SW_P6_KEEP").is_none() {
        let _ = fs::remove_dir_all(&root);
    }
    assert_eq!(len, 0, "network sides must not fill the store");
}
