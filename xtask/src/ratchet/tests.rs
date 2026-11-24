//! Tests for the ratchet text scan and its baseline comparison.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

const ABOVE: &str = "[lint_exceptions]\nmax = 2\n\n[test_sleeps]\nmax = 0\n";
const SLEEPS: &str = "[lint_exceptions]\nmax = 0\n\n[test_sleeps]\nmax = 2\n";
const BELOW: &str = "[lint_exceptions]\nmax = 5\n\n[test_sleeps]\nmax = 1\n";

// Split so the ratchet does not count these fixture strings as real sleeps.
const SLEEP: &str = concat!("std::thread::", "sleep", "(std::time::Duration::ZERO);");

/// A temporary git tree holding tracked `.rs` files and a ratchet config.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    /// Create a clean temp root with `xtask/ratchet.toml` set to `config`.
    fn new(name: &str, config: &str) -> Self {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("navo-ratchet-{name}-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("xtask")).unwrap();
        fs::write(root.join("xtask/ratchet.toml"), config).unwrap();
        Self { root }
    }

    /// Write one fixture source file, creating its parent directory.
    fn rs(&self, relative: &str, text: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    /// Add every fixture file to a fresh git index so `git ls-files` sees it.
    fn track(&self) {
        for args in [&["init", "-q"][..], &["add", "--", "."][..]] {
            let status = Command::new("git")
                .args(args)
                .current_dir(&self.root)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .status()
                .unwrap();
            assert!(status.success(), "git {args:?} failed");
        }
    }

    /// Run the ratchet against this fixture and return its code and stdout.
    fn run(&self) -> (ExitCode, String) {
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = super::run_in(&self.root, &mut out, &mut err);
        (code, String::from_utf8(out).unwrap())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn ratchet_fails_above_baseline() {
    let fixture = Fixture::new("above", ABOVE);
    fixture.rs(
        "src/a.rs",
        "#[allow(dead_code)]\n#[allow(unused_variables)]\n",
    );
    fixture.rs("src/b.rs", "#[expect(dead_code)]\n");
    fixture.track();
    let (code, out) = fixture.run();
    assert_eq!(code, ExitCode::from(1));
    assert!(out.contains("lint exceptions: 3 (max 2)"), "{out}");
    assert!(out.contains("src/a.rs: 2"), "{out}");
    assert!(out.contains("src/b.rs: 1"), "{out}");
}

#[test]
fn ratchet_counts_only_test_sleeps() {
    let fixture = Fixture::new("sleeps", SLEEPS);
    fixture.rs("src/a.rs", SLEEP);
    fixture.rs("src/a_tests.rs", SLEEP);
    let module = format!("#[cfg(test)]\nmod tests {{\n    fn t() {{ {SLEEP} }}\n}}\n");
    fixture.rs("src/b.rs", &module);
    fixture.track();
    let (code, out) = fixture.run();
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(out.contains("test sleeps: 2 (max 2)"), "{out}");
}

#[test]
fn ratchet_says_lower_the_baseline() {
    let fixture = Fixture::new("below", BELOW);
    fixture.rs("src/a.rs", "#[allow(dead_code, reason = \"fixture\")]\n");
    fixture.track();
    let (code, out) = fixture.run();
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(
        out.lines().any(|line| line == "lower the baseline to 1"),
        "{out}"
    );
    assert!(
        out.lines().any(|line| line == "lower the baseline to 0"),
        "{out}"
    );
}
