use super::*;

fn parse_strs(args: &[&str]) -> Result<Command, String> {
    parse(args.iter().map(OsString::from))
}

fn start(args: &[&str]) -> Start {
    match parse_strs(args) {
        Ok(Command::Open(start)) => start,
        other => panic!("expected a window start, got {other:?}"),
    }
}

fn refusal(args: &[&str]) -> String {
    parse_strs(args).expect_err("the arguments should be refused")
}

#[test]
fn no_arguments_open_the_window() {
    assert_eq!(parse_strs(&[]), Ok(Command::Open(Start::default())));
}

#[test]
fn retention_is_a_development_opt_in_for_window_starts_only() {
    assert!(!start(&[]).experimental_retention);
    assert!(start(&["--experimental-retention", "--profile", "p"]).experimental_retention);
    assert!(refusal(&["--experimental-retention", "--experimental-retention"]).contains("twice"));
    assert!(refusal(&["--version", "--experimental-retention"]).contains("stands alone"));
}

#[test]
fn version_flag_prints_the_version() {
    assert_eq!(parse_strs(&["--version"]), Ok(Command::Version));
}

#[test]
fn pdf_helper_mode_stands_alone_without_a_console() {
    assert_eq!(parse_strs(&["--pdf-helper"]), Ok(Command::PdfHelper));
    assert!(!Command::PdfHelper.uses_console());
    assert!(refusal(&["--pdf-helper", "a.pdf"]).contains("unknown argument a.pdf"));
    assert!(refusal(&["work", "--pdf-helper"]).contains("--pdf-helper stands alone"));
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
    assert!(refusal(&["--diag-report"]).contains("--diag-report needs a value"));
}

#[test]
fn crash_test_requires_an_explicit_profile_and_stands_alone() {
    assert_eq!(
        parse_strs(&["--crash-test", "probe"]),
        Ok(Command::CrashTest(PathBuf::from("probe")))
    );
    assert!(refusal(&["--crash-test"]).contains("--crash-test needs a value"));
    assert!(refusal(&["--crash-test", "probe", "--version"]).contains("stands alone"));
    assert!(refusal(&["work", "--crash-test", "probe"]).contains("stands alone"));
    assert!(!USAGE.contains("--crash-test"));
}

#[test]
fn an_empty_path_is_refused() {
    assert!(refusal(&["--diag-report", ""]).contains("--diag-report needs a value"));
}

#[test]
fn unknown_flags_are_refused() {
    assert!(refusal(&["--help"]).contains("unknown argument --help"));
}

#[test]
fn extra_arguments_are_refused() {
    assert!(refusal(&["--diag-report", "a.zip", "b"]).contains("unknown argument b"));
}

#[test]
fn every_refusal_ends_with_the_usage() {
    for args in [
        &["--help"][..],
        &["--metrics"],
        &["--left", "a", "--left", "b"],
        &["--version", "--metrics", "m.json"],
    ] {
        assert!(refusal(args).ends_with(USAGE), "{args:?}");
    }
}

#[test]
fn only_the_window_skips_the_console() {
    assert!(!Command::Open(Start::default()).uses_console());
    assert!(Command::Version.uses_console());
    assert!(Command::DiagReport(PathBuf::from("a.zip")).uses_console());
}

#[test]
fn view_takes_a_file_and_needs_no_console() {
    let parsed = start(&["--view", "notes.md"]);
    assert_eq!(parsed.view, Some(PathBuf::from("notes.md")));
    assert!(!Command::Open(parsed).uses_console());
    assert!(refusal(&["--view"]).contains("--view needs a value"));
}

#[test]
fn every_window_flag_is_read_in_any_order() {
    let parsed = start(&[
        "--right",
        "fixture/nested",
        "--metrics",
        "m.json",
        "--profile",
        "p",
        "--left",
        "fixture/flat",
    ]);
    assert_eq!(
        parsed,
        Start {
            experimental_retention: false,
            path: None,
            view: None,
            profile: Some(PathBuf::from("p")),
            metrics: Some(PathBuf::from("m.json")),
            left: Some(PathBuf::from("fixture/flat")),
            right: Some(PathBuf::from("fixture/nested")),
        }
    );
}

#[test]
fn one_positional_path_opens_the_window() {
    assert_eq!(start(&["work"]).path, Some(PathBuf::from("work")));
}

#[test]
fn the_positional_path_can_appear_between_window_flags() {
    let parsed = start(&["--left", "a", "work", "--right", "b"]);
    assert_eq!(parsed.path, Some(PathBuf::from("work")));
    assert_eq!(parsed.left, Some(PathBuf::from("a")));
    assert_eq!(parsed.right, Some(PathBuf::from("b")));
    assert_eq!(
        start(&["work", "--view", "note"]).view,
        Some(PathBuf::from("note"))
    );
}

#[test]
fn two_positional_paths_are_refused() {
    assert!(refusal(&["a", "b"]).contains("only one path"));
}

#[test]
fn a_path_cannot_accompany_a_standalone_flag() {
    for args in [
        &["work", "--version"][..],
        &["--version", "work"],
        &["work", "--diag-report", "out.zip"],
        &["--diag-report", "out.zip", "work"],
    ] {
        assert!(parse_strs(args).is_err(), "{args:?}");
    }
}

#[test]
fn metrics_mode_reports_to_the_console() {
    let parsed = start(&["--metrics", "m.json"]);
    assert!(parsed.is_metrics());
    assert!(Command::Open(parsed).uses_console());
}

#[test]
fn a_profile_alone_opens_the_window_there() {
    let parsed = start(&["--profile", "p"]);
    assert_eq!(parsed.profile, Some(PathBuf::from("p")));
    assert!(!parsed.is_metrics());
}

#[test]
fn a_window_flag_without_a_value_is_refused() {
    for flag in ["--metrics", "--profile", "--left", "--right"] {
        let needs = format!("{flag} needs a value");
        assert!(refusal(&[flag]).contains(&needs), "{flag}");
        assert!(refusal(&[flag, ""]).contains(&needs), "{flag}");
    }
}

#[test]
fn a_flag_is_not_taken_as_a_value() {
    assert!(refusal(&["--metrics", "--left", "a"]).contains("--metrics needs a value"));
}

#[test]
fn a_flag_given_twice_is_refused() {
    let message = refusal(&["--metrics", "a.json", "--metrics", "b.json"]);
    assert!(message.contains("--metrics given twice"));
}

#[test]
fn version_and_diag_report_stand_alone() {
    assert!(refusal(&["--version", "--left", "a"]).contains("--version stands alone"));
    let message = refusal(&["--left", "a", "--diag-report", "r.zip"]);
    assert!(message.contains("--diag-report stands alone"));
}

#[cfg(unix)]
#[test]
fn a_path_that_is_not_unicode_is_kept() {
    use std::os::unix::ffi::OsStringExt;
    let odd = OsString::from_vec(vec![b'f', 0xff]);
    let parsed = parse([OsString::from("--left"), odd.clone()]);
    let Ok(Command::Open(start)) = parsed else {
        panic!("expected a window start");
    };
    assert_eq!(start.left, Some(PathBuf::from(odd)));
}

#[cfg(unix)]
#[test]
fn a_flag_that_is_not_unicode_is_refused_without_a_panic() {
    use std::os::unix::ffi::OsStringExt;
    let odd = OsString::from_vec(vec![b'-', b'-', 0xff]);
    assert!(parse([odd]).is_err());
}

#[test]
fn bench_is_a_console_subcommand_only_in_first_place() {
    let Ok(command) = parse_strs(&["bench", "--root", "fixture", "--files", "5000"]) else {
        panic!("bench should parse");
    };
    assert!(command.uses_console());
    assert!(matches!(command, Command::Bench(options) if options.files == 5_000));
    assert_eq!(
        start(&["--left", "a", "bench"]).path,
        Some(PathBuf::from("bench"))
    );
    assert!(refusal(&["bench"]).contains("--root"));
}

#[test]
fn index_bench_accepts_plain_text_and_json_as_console_commands() {
    for args in [vec!["--bench", "index"], vec!["--bench", "index", "--json"]] {
        let command = parse_strs(&args).unwrap();
        assert!(command.uses_console());
        assert_eq!(
            command,
            Command::BenchIndex {
                json: args.contains(&"--json"),
            }
        );
    }
}

#[test]
fn index_bench_rejects_missing_modes_and_unrelated_arguments() {
    for args in [
        vec!["--bench"],
        vec!["--bench", "other"],
        vec!["--bench", "index", "--root", "fixture"],
        vec!["--bench", "index", "--json", "--json"],
        vec!["--bench", "index", "extra"],
        vec!["--left", "fixture", "--bench", "index"],
        vec!["--json", "--bench", "index"],
    ] {
        assert!(parse_strs(&args).is_err(), "{args:?}");
    }
}
