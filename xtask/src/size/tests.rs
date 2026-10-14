use super::*;
use std::path::PathBuf;

fn code(n: usize) -> String {
    "let x = 1;\n".repeat(n)
}

fn file(path: &str, source: String) -> (PathBuf, String) {
    (PathBuf::from(path), source)
}

fn exception(path: &str, limit: usize) -> Exception {
    Exception {
        path: PathBuf::from(path),
        limit,
        reason: "state machine".to_string(),
    }
}

#[test]
fn blank_and_whitespace_lines_are_not_counted() {
    assert_eq!(count_code_lines("a;\n\n   \n\t\nb;\n"), 2);
}

#[test]
fn line_comments_are_not_counted() {
    let source = "// one\n    /// two\n\t//! three\nlet x = 1;\n";
    assert_eq!(count_code_lines(source), 1);
}

#[test]
fn block_comment_lines_are_not_counted() {
    let source = "a;\n/* start\nmiddle\nend */\nb;\n";
    assert_eq!(count_code_lines(source), 2);
}

#[test]
fn nested_block_comments_spanning_lines_are_not_counted() {
    let source = "a;\n/* outer\n/* inner\n*/\nstill outer */\nb;\n";
    assert_eq!(count_code_lines(source), 2);
}

#[test]
fn code_with_trailing_comment_is_counted() {
    assert_eq!(count_code_lines("let x = 1; // note\n"), 1);
}

#[test]
fn string_literal_with_slashes_is_counted() {
    assert_eq!(count_code_lines("let u = \"http://x\";\n"), 1);
}

#[test]
fn file_over_the_default_limit_is_a_violation() {
    let files = [file("src/big.rs", code(401))];
    let expected = SizeViolation {
        path: PathBuf::from("src/big.rs"),
        lines: 401,
        limit: 400,
    };
    assert_eq!(check_files(&files, &[]), vec![expected]);
}

#[test]
fn file_at_the_default_limit_is_not_a_violation() {
    let files = [file("src/ok.rs", code(400))];
    assert!(check_files(&files, &[]).is_empty());
}

#[test]
fn exception_raises_the_limit_for_its_own_path_only() {
    let files = [file("src/a.rs", code(500)), file("src/b.rs", code(500))];
    let violations = check_files(&files, &[exception("src/a.rs", 600)]);
    let expected = SizeViolation {
        path: PathBuf::from("src/b.rs"),
        lines: 500,
        limit: 400,
    };
    assert_eq!(violations, vec![expected]);
}

#[test]
fn exception_limit_is_still_enforced() {
    let files = [file("src/a.rs", code(601))];
    let violations = check_files(&files, &[exception("src/a.rs", 600)]);
    let expected = SizeViolation {
        path: PathBuf::from("src/a.rs"),
        lines: 601,
        limit: 600,
    };
    assert_eq!(violations, vec![expected]);
}

#[test]
fn violations_follow_input_order() {
    let files = [file("src/z.rs", code(401)), file("src/a.rs", code(402))];
    let paths: Vec<PathBuf> = check_files(&files, &[])
        .into_iter()
        .map(|v| v.path)
        .collect();
    assert_eq!(
        paths,
        vec![PathBuf::from("src/z.rs"), PathBuf::from("src/a.rs")]
    );
}

#[test]
fn parse_reads_two_entries() {
    let toml = r#"
[[exception]]
path = "crates/a/src/parser.rs"
limit = 600
reason = "one parser"

[[exception]]
path = "crates/b/src/table.rs"
limit = 800
reason = "lookup table"
"#;
    let expected = vec![
        Exception {
            path: PathBuf::from("crates/a/src/parser.rs"),
            limit: 600,
            reason: "one parser".to_string(),
        },
        Exception {
            path: PathBuf::from("crates/b/src/table.rs"),
            limit: 800,
            reason: "lookup table".to_string(),
        },
    ];
    assert_eq!(parse_exceptions(toml).unwrap(), expected);
}

fn entry(limit: usize) -> String {
    format!("[[exception]]\npath = \"a.rs\"\nlimit = {limit}\nreason = \"r\"\n")
}

#[test]
fn limit_above_ceiling_is_rejected() {
    let error = parse_exceptions(&entry(801)).unwrap_err();
    let expected = SizeError::LimitAboveCeiling {
        path: PathBuf::from("a.rs"),
        limit: 801,
    };
    assert_eq!(error, expected);
}

#[test]
fn limit_at_ceiling_is_accepted() {
    assert_eq!(parse_exceptions(&entry(800)).unwrap().len(), 1);
}

#[test]
fn missing_reason_is_rejected() {
    let toml = "[[exception]]\npath = \"a.rs\"\nlimit = 500\n";
    let error = parse_exceptions(toml).unwrap_err();
    assert!(matches!(error, SizeError::MissingReason { .. }));
}

#[test]
fn blank_reason_is_rejected() {
    let toml = "[[exception]]\npath = \"a.rs\"\nlimit = 500\nreason = \"  \"\n";
    let error = parse_exceptions(toml).unwrap_err();
    assert_eq!(
        error,
        SizeError::MissingReason {
            path: PathBuf::from("a.rs")
        }
    );
}

#[test]
fn malformed_toml_is_rejected() {
    let error = parse_exceptions("[[exception\npath = ").unwrap_err();
    assert!(matches!(error, SizeError::InvalidToml(_)));
}

#[test]
fn files_named_tests_rs_are_skipped() {
    let files = [file("crates/rusty-core/src/tests.rs", code(900))];
    assert!(check_files(&files, &[]).is_empty());
}

#[test]
fn files_under_a_tests_folder_are_skipped() {
    let files = [file("crates/rusty-core/tests/big.rs", code(900))];
    assert!(check_files(&files, &[]).is_empty());
}

#[test]
fn raw_strings_preserve_comment_markers() {
    let source = "let a = r\"//\";\nlet b = r\"/*\";\n// skipped\n";
    assert_eq!(count_code_lines(source), 2);
}

#[test]
fn hashed_raw_strings_preserve_quotes_and_comment_markers() {
    let source = r##"let a = r#""//"#;
let b = r#""/*"#;
// skipped
"##;
    assert_eq!(count_code_lines(source), 2);
}

#[test]
fn byte_raw_strings_preserve_comment_markers() {
    let source = "let a = br\"//\";\nlet b = br#\"/*\"#;\n// skipped\n";
    assert_eq!(count_code_lines(source), 2);
}

#[test]
fn raw_strings_spanning_lines_count_their_contents() {
    let source = "let a = r\"start\n// text\n/* text */\n\";\n// skipped\n";
    assert_eq!(count_code_lines(source), 4);
}

#[test]
fn hashed_raw_strings_require_the_matching_closing_hashes() {
    let source = "let a = r##\"start\n\"# // text\n/* text */\n\"##;\n// skipped\n";
    assert_eq!(count_code_lines(source), 4);
}

#[test]
fn escaped_quotes_do_not_end_strings() {
    let source = "let a = \"a\\\"//b\";\n// skipped\n";
    assert_eq!(count_code_lines(source), 1);
}

#[test]
fn strings_spanning_lines_count_comment_markers_as_text() {
    let source = "let a = \"start\n// text\n/* text */\n\";\n// skipped\n";
    assert_eq!(count_code_lines(source), 4);
}

#[test]
fn quote_and_slash_char_literals_do_not_change_lexer_state() {
    let source = "let a = '\"';\nlet b = '\\'';\nlet c = '/';\n// skipped\n";
    assert_eq!(count_code_lines(source), 3);
}

#[test]
fn lifetimes_are_code_and_do_not_start_literals() {
    let source = "fn f<'a>(x: &'a str) {}\n'a\n'\n// skipped\n";
    assert_eq!(count_code_lines(source), 3);
}

#[test]
fn block_comment_opening_after_code_counts_only_the_code_line() {
    assert_eq!(count_code_lines("x; /* c\n*/\n"), 1);
}

#[test]
fn invalid_size_exceptions_display_the_parse_message() {
    let error = SizeError::InvalidToml("bad syntax".to_owned());
    assert_eq!(error.to_string(), "invalid size exceptions: bad syntax");
}

#[test]
fn excessive_size_exception_displays_the_path_limit_and_ceiling() {
    let error = SizeError::LimitAboveCeiling {
        path: PathBuf::from("src/large.rs"),
        limit: 801,
    };
    assert_eq!(
        error.to_string(),
        "src/large.rs: exception limit 801 exceeds ceiling 800"
    );
}

#[test]
fn missing_size_exception_reason_displays_the_path() {
    let error = SizeError::MissingReason {
        path: PathBuf::from("src/large.rs"),
    };
    assert_eq!(
        error.to_string(),
        "src/large.rs: exception requires a non-blank reason"
    );
}

struct TestTree(PathBuf);

impl TestTree {
    fn new() -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("rusty-size-{}-{timestamp}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("crates")).unwrap();
        std::fs::create_dir(root.join("xtask")).unwrap();
        Self(root)
    }

    fn write(&self, path: &str, source: impl AsRef<[u8]>) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
}

impl Drop for TestTree {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn walk_sorts_rust_sources_and_skips_build_test_and_other_files() {
    let tree = TestTree::new();
    tree.write("xtask/src/main.rs", "fn main() {}\n");
    tree.write("crates/a/src/lib.rs", "pub fn a() {}\n");
    for path in [
        "crates/a/target/big.rs",
        "crates/a/tests/big.rs",
        "crates/a/src/tests.rs",
        "xtask/src/readme.txt",
    ] {
        tree.write(path, [0xff]);
    }
    assert_eq!(
        run::read_files(&tree.0).unwrap(),
        vec![
            file("crates/a/src/lib.rs", "pub fn a() {}\n".to_owned()),
            file("xtask/src/main.rs", "fn main() {}\n".to_owned()),
        ]
    );
}

#[test]
fn root_check_rejects_a_401_line_source() {
    let tree = TestTree::new();
    tree.write("crates/a/src/lib.rs", code(401));
    assert!(!run::run_check(&tree.0).unwrap());
}

#[test]
fn root_check_applies_the_repository_exception_file() {
    let tree = TestTree::new();
    tree.write("crates/a/src/lib.rs", code(401));
    tree.write(
        "size-exceptions.toml",
        "[[exception]]\npath = \"crates/a/src/lib.rs\"\nlimit = 401\nreason = \"state machine\"\n",
    );
    assert!(run::run_check(&tree.0).unwrap());
}

#[test]
fn root_check_reports_invalid_exceptions() {
    let tree = TestTree::new();
    tree.write("size-exceptions.toml", "[[exception");
    let error = run::run_check(&tree.0).unwrap_err();
    assert!(error.starts_with("invalid size exceptions:"));
}

#[test]
fn root_check_reports_unreadable_exceptions() {
    let tree = TestTree::new();
    tree.write("size-exceptions.toml", [0xff]);
    let error = run::run_check(&tree.0).unwrap_err();
    assert!(error.starts_with(&format!(
        "{}:",
        tree.0.join("size-exceptions.toml").display()
    )));
}

#[test]
fn root_check_reports_a_missing_source_directory() {
    let tree = TestTree::new();
    std::fs::remove_dir(tree.0.join("crates")).unwrap();
    let error = run::run_check(&tree.0).unwrap_err();
    assert!(error.starts_with(&format!("{}:", tree.0.join("crates").display())));
}

#[test]
fn root_check_reports_non_utf8_rust_sources() {
    let tree = TestTree::new();
    tree.write("xtask/src/main.rs", [0xff]);
    let error = run::run_check(&tree.0).unwrap_err();
    let source = tree.0.join("xtask").join("src").join("main.rs");
    assert!(error.starts_with(&format!("{}:", source.display())));
}
