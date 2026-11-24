//! Checks the Windows command-line quoting copied from `navo-shell` (H3).

use crate::cmdline::command_line;

/// Builds the command line from `&str` parts and returns it as a `String`.
fn line(program: &str, args: &[&str]) -> String {
    let program: Vec<u16> = program.encode_utf16().collect();
    let args: Vec<Vec<u16>> = args
        .iter()
        .map(|arg| arg.encode_utf16().collect())
        .collect();
    String::from_utf16(&command_line(&program, args.iter().map(Vec::as_slice))).unwrap()
}

#[test]
fn plain_arguments_stay_bare_and_the_program_is_always_quoted() {
    assert_eq!(
        line(r"C:\Program Files\navo.exe", &["--pdf-helper"]),
        r#""C:\Program Files\navo.exe" --pdf-helper"#
    );
}

#[test]
fn spaces_tabs_and_empty_arguments_are_quoted() {
    assert_eq!(
        line("a", &["b c", "d\te", ""]),
        "\"a\" \"b c\" \"d\te\" \"\""
    );
}

#[test]
fn quotes_and_the_backslashes_before_them_are_escaped() {
    assert_eq!(line("a", &[r#"say "hi""#]), r#""a" "say \"hi\"""#);
    assert_eq!(line("a", &[r#"x\"y"#]), r#""a" "x\\\"y""#);
}

#[test]
fn trailing_backslashes_are_doubled_only_inside_quotes() {
    assert_eq!(line("a", &[r"C:\dir\"]), r#""a" C:\dir\"#);
    assert_eq!(line("a", &[r"C:\my dir\"]), r#""a" "C:\my dir\\""#);
    assert_eq!(line("a", &[r"a\\b"]), r#""a" a\\b"#);
}
