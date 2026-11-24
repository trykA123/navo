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
        "use futures::channel::mpsc::{unbounded, unbounded_channel};\nfn f() {\n    let _ = unbounded();\n    let _ = unbounded_channel();\n    let _ = mpsc::channel();\n}\n",
    );
    let hits = tree.scan(&["src/a.rs"]);
    assert_eq!(hits.len(), 3);
    assert_eq!(lines(&hits), vec![3, 4, 5]);
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
    tree.write(
        "src/scan_tests/limits.rs",
        "let (a, b) = mpsc::channel();\n",
    );
    tree.write("src/tests/helper.rs", "let (a, b) = mpsc::channel();\n");
    tree.write(
        "src/cfg.rs",
        "let (a, b) = mpsc::channel();\n#[cfg(test)]\nmod tests {\n    let (a, b) = mpsc::channel();\n}\n",
    );
    let hits = tree.scan(&[
        "src/bounded.rs",
        "src/tests.rs",
        "src/worker_tests.rs",
        "src/scan_tests/limits.rs",
        "src/tests/helper.rs",
        "src/cfg.rs",
    ]);
    assert_eq!(hits.len(), 1);
    assert_eq!(lines(&hits), vec![1]);
}

#[test]
fn channels_after_test_module_declaration_are_counted() {
    let tree = Tree::new("after-declaration");
    tree.write(
        "src/a.rs",
        "#[cfg(test)]\nmod tests;\n\nlet (a, b) = mpsc::channel();\n",
    );
    let hits = tree.scan(&["src/a.rs"]);
    assert_eq!(lines(&hits), vec![4]);
}

#[test]
fn channels_inside_inline_test_module_are_skipped() {
    let tree = Tree::new("inline");
    tree.write(
        "src/a.rs",
        "#[cfg(test)]\nmod tests {\n    const A: &str = \"}\";\n    const B: char = '{';\n    /* } */\n    let (a, b) = mpsc::channel();\n}\n",
    );
    let hits = tree.scan(&["src/a.rs"]);
    assert!(hits.is_empty(), "{hits:?}");
}

#[test]
fn channels_after_inline_test_module_are_counted() {
    let tree = Tree::new("after-inline");
    tree.write(
        "src/a.rs",
        "#[cfg(test)]\nmod tests {\n    fn t() {\n        let (a, b) = mpsc::channel();\n    }\n}\n\nfn live() {\n    let (a, b) = mpsc::channel();\n}\n",
    );
    let hits = tree.scan(&["src/a.rs"]);
    assert_eq!(lines(&hits), vec![9]);
}

#[test]
fn channels_skip_test_items_but_not_later_code() {
    let tree = Tree::new("items");
    tree.write(
        "src/a.rs",
        "#[cfg(test)]\nuse a::b;\n#[cfg(test)]\nfn helper(a: u8, b: u8) {\n    let (c, d) = mpsc::channel();\n}\n#[cfg(test)]\nimpl X {\n    fn h(&self) { let (a, b) = mpsc::channel(); }\n}\nlet (e, f) = mpsc::channel();\n",
    );
    let hits = tree.scan(&["src/a.rs"]);
    assert_eq!(lines(&hits), vec![11]);
}

#[test]
fn channels_after_test_parameter_are_counted() {
    let tree = Tree::new("parameter");
    tree.write(
        "src/a.rs",
        "fn build(#[cfg(test)] hook: Option<u8>, live: u8) {\n    let (a, b) = mpsc::channel();\n}\n",
    );
    let hits = tree.scan(&["src/a.rs"]);
    assert_eq!(lines(&hits), vec![2]);
}

#[test]
fn channels_in_comments_and_strings_are_not_counted() {
    let tree = Tree::new("masked");
    tree.write(
        "src/a.rs",
        "// let (a, b) = mpsc::channel();\nconst S: &str = \"unbounded()\";\n/* let (c, d) = unbounded(); */\nlet (e, f) = mpsc::channel();\n",
    );
    let hits = tree.scan(&["src/a.rs"]);
    assert_eq!(lines(&hits), vec![4]);
}

#[test]
fn channels_share_a_line_with_test_items() {
    let tree = Tree::new("shared-line");
    tree.write(
        "src/a.rs",
        "fn f() { #[cfg(test)] let hook = (); let _ = mpsc::channel(); }\n#[cfg(test)] fn t() { let _ = mpsc::channel(); } let _ = mpsc::channel();\n",
    );
    let hits = tree.scan(&["src/a.rs"]);
    assert_eq!(lines(&hits), vec![1, 2]);
}

#[test]
fn channels_skip_stacked_attributes_and_where_clauses() {
    let tree = Tree::new("stacked");
    tree.write(
        "src/a.rs",
        "#[cfg(test)]\n#[allow(dead_code)]\nfn helper<T>(value: T) -> T\nwhere\n    T: Copy,\n{\n    let (a, b) = mpsc::channel();\n}\nlet (c, d) = mpsc::channel();\n",
    );
    let hits = tree.scan(&["src/a.rs"]);
    assert_eq!(lines(&hits), vec![9]);
}

#[test]
fn channels_read_compound_cfgs() {
    let tree = Tree::new("compound");
    tree.write(
        "src/a.rs",
        "#[cfg( test )]\nfn spaced() { let (a, b) = mpsc::channel(); }\n#[cfg(all(test, unix))]\nfn all_test() { let (a, b) = mpsc::channel(); }\n#[cfg(any(test, feature = \"test-support\"))]\nfn any_test() { let (c, d) = mpsc::channel(); }\n#[cfg(not(test))]\nfn not_test() { let (e, f) = mpsc::channel(); }\n",
    );
    let hits = tree.scan(&["src/a.rs"]);
    assert_eq!(lines(&hits), vec![6, 8]);
}

#[test]
fn channels_skip_braces_in_attributes_and_code() {
    let tree = Tree::new("braces");
    tree.write(
        "src/a.rs",
        "#[cfg(test)]\n#[doc = stringify!{x}]\nfn t<T>() where T: Trait<{1}> {\n    let (a, b) = mpsc::channel();\n}\nlet (c, d) = mpsc::channel();\n",
    );
    let hits = tree.scan(&["src/a.rs"]);
    assert_eq!(lines(&hits), vec![6]);
}

#[test]
fn channels_skip_raw_strings_and_nested_comments() {
    let tree = Tree::new("raw");
    tree.write(
        "src/a.rs",
        "#[cfg(test)]\n/* outer /* inner */ } */\nfn t<'a>(value: &'a str) {\n    let doc = r#\"}\"#;\n    let (a, b) = mpsc::channel();\n}\nlet (c, d) = mpsc::channel();\n",
    );
    let hits = tree.scan(&["src/a.rs"]);
    assert_eq!(lines(&hits), vec![7]);
}

#[test]
fn channels_count_turbofish_constructors_only() {
    let tree = Tree::new("turbofish");
    tree.write(
        "src/a.rs",
        "use futures::channel::mpsc::unbounded;\nfn f() {\n    let _ = unbounded::<u8>();\n    let _ = futures::channel::mpsc::unbounded::<u8>();\n    let _ = mpsc::channel::<u8>();\n    let _ = not_unbounded();\n    let _ = other::unbounded();\n}\n",
    );
    tree.write("src/b.rs", "fn f() { let _ = unbounded(); }\n");
    let hits = tree.scan(&["src/a.rs", "src/b.rs"]);
    assert_eq!(lines(&hits), vec![3, 4, 5]);
}

#[test]
fn channels_count_only_unbounded_imported_channel() {
    let tree = Tree::new("bare-channel");
    tree.write(
        "src/a.rs",
        "use std::sync::mpsc::channel;\nfn f() { let (a, b) = channel(); }\n",
    );
    tree.write(
        "src/b.rs",
        "use tokio::sync::mpsc::channel;\nfn f() { let (a, b) = channel(4); }\n",
    );
    tree.write(
        "src/c.rs",
        "use std::sync::mpsc::channel;\nfn f(hooks: Hooks) { let (a, b) = hooks.channel(); }\n",
    );
    let hits = tree.scan(&["src/a.rs", "src/b.rs", "src/c.rs"]);
    assert_eq!(lines(&hits), vec![2]);
}

#[test]
fn channels_skip_inner_cfg_test_files_and_blocks() {
    let tree = Tree::new("inner");
    tree.write(
        "src/a.rs",
        "#![cfg(test)]\nfn a() { let (a, b) = mpsc::channel(); }\nfn b() { let (c, d) = mpsc::channel(); }\n",
    );
    tree.write(
        "src/b.rs",
        "mod tests {\n    #![cfg(test)]\n    fn a() { let (a, b) = mpsc::channel(); }\n    fn b() { let (c, d) = mpsc::channel(); }\n}\nlet (e, f) = mpsc::channel();\n",
    );
    let hits = tree.scan(&["src/a.rs", "src/b.rs"]);
    assert_eq!(lines(&hits), vec![6]);
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
