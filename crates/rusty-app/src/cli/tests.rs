use super::*;

fn parse_strs(args: &[&str]) -> Result<Command, &'static str> {
    parse(args.iter().map(OsString::from))
}

#[test]
fn no_arguments_open_the_window() {
    assert_eq!(parse_strs(&[]), Ok(Command::Open));
}

#[test]
fn version_flag_prints_the_version() {
    assert_eq!(parse_strs(&["--version"]), Ok(Command::Version));
}

#[test]
fn diag_report_takes_a_path() {
    assert_eq!(
        parse_strs(&["--diag-report", "out.zip"]),
        Ok(Command::DiagReport(PathBuf::from("out.zip")))
    );
}

#[test]
fn diag_report_without_a_path_is_refused() {
    assert_eq!(parse_strs(&["--diag-report"]), Err(USAGE));
}

#[test]
fn an_empty_path_is_refused() {
    assert_eq!(parse_strs(&["--diag-report", ""]), Err(USAGE));
}

#[test]
fn unknown_flags_are_refused() {
    assert_eq!(parse_strs(&["--help"]), Err(USAGE));
}

#[test]
fn extra_arguments_are_refused() {
    assert_eq!(parse_strs(&["--diag-report", "a.zip", "b"]), Err(USAGE));
}

#[test]
fn only_the_window_skips_the_console() {
    assert!(!Command::Open.uses_console());
    assert!(Command::Version.uses_console());
    assert!(Command::DiagReport(PathBuf::from("a.zip")).uses_console());
}

#[test]
fn view_takes_a_file_and_needs_no_console() {
    let parsed = parse_strs(&["--view", "notes.md"]);
    assert_eq!(parsed, Ok(Command::View(PathBuf::from("notes.md"))));
    assert!(!Command::View(PathBuf::from("a")).uses_console());
    assert_eq!(parse_strs(&["--view"]), Err(USAGE));
}
