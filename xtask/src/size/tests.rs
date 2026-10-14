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
