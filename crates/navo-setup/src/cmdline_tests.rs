//! Checks the Windows command-line quoting copied from `navo-shell` (H3).

use crate::cmdline::{command_line, is_absolute};

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

/// Whether `path` is an absolute Windows path, as `is_absolute` reads it.
fn absolute(path: &str) -> bool {
    is_absolute(&path.encode_utf16().collect::<Vec<u16>>())
}

#[test]
fn drive_and_unc_paths_are_absolute() {
    assert!(absolute(r"C:\Windows\System32\cmd.exe"));
    assert!(absolute("c:/tools/navo.exe"));
    assert!(absolute(r"\\server\share\navo.exe"));
    assert!(absolute(r"\\?\C:\long\navo.exe"));
    assert!(absolute(r"\\server\share"));
    assert!(absolute("//server/share/navo.exe"));
}

#[test]
fn unc_paths_without_a_share_are_not_absolute() {
    assert!(!absolute(r"\\server"));
    assert!(!absolute(r"\\server\"));
    assert!(!absolute(r"\\server\\share"));
}

#[test]
fn relative_drive_relative_and_rooted_paths_are_not_absolute() {
    assert!(!absolute("navo.exe"));
    assert!(!absolute(r".\navo.exe"));
    assert!(!absolute(r"..\navo.exe"));
    assert!(!absolute("C:navo.exe"));
    assert!(!absolute(r"\Windows\navo.exe"));
    assert!(!absolute(r"\\"));
    assert!(!absolute("1:\\navo.exe"));
    assert!(!absolute(""));
}
