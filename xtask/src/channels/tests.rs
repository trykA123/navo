//! Unit tests for the channel ratchet (AM7).

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{Hit, report, scan};

/// A fresh temp tree that removes itself when the test ends.
struct Tree(PathBuf);

impl Tree {
    /// Create an empty tree named after the test and a process-wide counter.
    fn new(name: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "navo-channels-{}-{}-{}",
            std::process::id(),
            name,
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    /// Write `text` to the repo-relative path `rel`, creating its folders.
    fn write(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, text).unwrap();
    }

    /// Scan the listed repo-relative files in this tree.
    fn scan(&self, files: &[&str]) -> Vec<Hit> {
        let files: Vec<String> = files.iter().map(|file| (*file).to_string()).collect();
        scan(&self.0, &files).unwrap()
    }

    /// Run the reporter on `hits` and return its exit code and printed text.
    fn report(&self, hits: &[Hit], baseline: usize) -> (ExitCode, String) {
        let mut out = Vec::new();
        let code = report(hits, baseline, &mut out).unwrap();
        (code, String::from_utf8(out).unwrap())
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Return the 1-based lines of `hits`, in order.
fn lines(hits: &[Hit]) -> Vec<usize> {
    hits.iter().map(|hit| hit.line).collect()
}

#[test]
fn channels_fail_above_baseline() {
    let tree = Tree::new("above");
    tree.write(
        "src/a.rs",
        "fn f() {\n    let _ = unbounded();\n    let _ = unbounded_channel();\n    let _ = mpsc::channel();\n}\n",
    );
    let hits = tree.scan(&["src/a.rs"]);
    assert_eq!(hits.len(), 3);
    assert_eq!(lines(&hits), vec![2, 3, 4]);
    let (code, out) = tree.report(&hits, 2);
    assert_eq!(code, ExitCode::FAILURE);
    assert_eq!(
        out.lines()
            .filter(|line| line.starts_with("src/a.rs:"))
            .count(),
        3
    );
}

#[test]
fn channels_skip_tests_and_bounded() {
    let tree = Tree::new("skip");
    tree.write("src/bounded.rs", "let (a, b) = mpsc::sync_channel(8);\n");
    tree.write("src/tests.rs", "let (a, b) = mpsc::channel();\n");
    tree.write("src/worker_tests.rs", "let (a, b) = unbounded();\n");
    tree.write("src/tests/helper.rs", "let (a, b) = mpsc::channel();\n");
    tree.write(
        "src/cfg.rs",
        "let (a, b) = mpsc::channel();\n#[cfg(test)]\nmod tests {\n    let (a, b) = mpsc::channel();\n}\n",
    );
    let hits = tree.scan(&[
        "src/bounded.rs",
        "src/tests.rs",
        "src/worker_tests.rs",
        "src/tests/helper.rs",
        "src/cfg.rs",
    ]);
    assert_eq!(hits.len(), 1);
    assert_eq!(lines(&hits), vec![1]);
}

#[test]
fn channels_say_lower_the_baseline() {
    let tree = Tree::new("lower");
    tree.write("src/a.rs", "let (a, b) = mpsc::channel();\n");
    let hits = tree.scan(&["src/a.rs"]);
    let (code, out) = tree.report(&hits, 5);
    assert_eq!(code, ExitCode::SUCCESS);
    assert!(out.contains("lower the baseline to 1"), "{out}");
}
