//! Tests of the two Explorer pick flags and their refusal beside sides.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::args::{Action, ArgError, Invocation, parse};

/// Turns string literals into the OsString list the parser takes.
fn to_os(items: &[&str]) -> Vec<OsString> {
    items.iter().map(OsString::from).collect()
}

/// Builds a bare invocation around one action, every flag at its default.
fn bare(action: Action) -> Invocation {
    Invocation {
        action,
        wait: false,
        new_window: false,
        read_only: false,
        profile: None,
        left_title: None,
        right_title: None,
    }
}

/// Parses string literals from the fixed working folder `cwd`.
fn parse_with(items: &[&str], exists: &dyn Fn(&Path) -> bool) -> Result<Invocation, ArgError> {
    parse(&to_os(items), Path::new("cwd"), exists)
}

/// Reports that no path exists.
fn nothing(_: &Path) -> bool {
    false
}

/// Returns the `cwd`-joined path used for a pick value.
fn picked(text: &str) -> PathBuf {
    Path::new("cwd").join(text)
}

/// Each pick flag accepts the separate value and the `=` form.
#[test]
fn each_pick_flag_accepts_both_spellings() {
    assert_eq!(
        parse_with(&["--select-left", "left"], &nothing),
        Ok(bare(Action::SelectLeft {
            path: picked("left")
        }))
    );
    assert_eq!(
        parse_with(&["--select-left=left"], &nothing),
        Ok(bare(Action::SelectLeft {
            path: picked("left")
        }))
    );
    assert_eq!(
        parse_with(&["--compare-with", "right"], &nothing),
        Ok(bare(Action::CompareWith {
            right: picked("right")
        }))
    );
    assert_eq!(
        parse_with(&["--compare-with=right"], &nothing),
        Ok(bare(Action::CompareWith {
            right: picked("right")
        }))
    );
}

/// A relative value joins `cwd` without being resolved.
#[test]
fn a_relative_value_joins_cwd() {
    assert_eq!(
        parse_with(&["--select-left", "../left"], &nothing),
        Ok(bare(Action::SelectLeft {
            path: picked("../left")
        }))
    );
    assert_eq!(
        parse_with(&["--compare-with=./right"], &nothing),
        Ok(bare(Action::CompareWith {
            right: picked("./right")
        }))
    );
}

/// A `repo@rev` value stays one plain path and the existence test is never called.
#[test]
fn a_repo_at_rev_value_is_not_parsed_and_exists_is_not_called() {
    let calls = AtomicUsize::new(0);
    let exists = |path: &Path| {
        calls.fetch_add(1, Ordering::Relaxed);
        path == Path::new("cwd/repo")
    };
    assert_eq!(
        parse_with(&["--select-left", "repo@main"], &exists),
        Ok(bare(Action::SelectLeft {
            path: picked("repo@main")
        }))
    );
    assert_eq!(
        parse_with(&["--compare-with", "repo@main"], &exists),
        Ok(bare(Action::CompareWith {
            right: picked("repo@main")
        }))
    );
    assert_eq!(calls.load(Ordering::Relaxed), 0);
}

/// A missing or empty value names the flag that needs it.
#[test]
fn missing_and_empty_values_are_refused_with_the_flag_name() {
    assert_eq!(
        parse_with(&["--select-left"], &nothing),
        Err(ArgError::MissingValue("--select-left"))
    );
    assert_eq!(
        parse_with(&["--compare-with"], &nothing),
        Err(ArgError::MissingValue("--compare-with"))
    );
    assert_eq!(
        parse_with(&["--select-left="], &nothing),
        Err(ArgError::MissingValue("--select-left"))
    );
    assert_eq!(
        parse_with(&["--compare-with="], &nothing),
        Err(ArgError::MissingValue("--compare-with"))
    );
    assert_eq!(
        parse_with(&["--select-left", ""], &nothing),
        Err(ArgError::MissingValue("--select-left"))
    );
    assert_eq!(
        parse_with(&["--compare-with", ""], &nothing),
        Err(ArgError::MissingValue("--compare-with"))
    );
}

/// A repeated pick flag keeps its last value.
#[test]
fn a_repeated_pick_flag_keeps_the_last_value() {
    assert_eq!(
        parse_with(&["--select-left", "a", "--select-left=b"], &nothing),
        Ok(bare(Action::SelectLeft { path: picked("b") }))
    );
    assert_eq!(
        parse_with(&["--compare-with=a", "--compare-with", "b"], &nothing),
        Ok(bare(Action::CompareWith { right: picked("b") }))
    );
}

/// Both pick flags, or a pick flag with any positional, are refused together.
#[test]
fn pick_flags_refuse_a_second_side() {
    for items in [
        vec!["--select-left", "left", "--compare-with", "right"],
        vec!["--compare-with", "right", "--select-left=left"],
        vec!["--select-left", "left", "side"],
        vec!["side", "--select-left=left"],
        vec!["--compare-with", "right", "side"],
        vec!["side", "--compare-with=right"],
    ] {
        assert_eq!(parse_with(&items, &nothing), Err(ArgError::PickWithSides));
    }
}

/// The other flags stay allowed beside a pick flag and reach the invocation.
#[test]
fn other_flags_stay_allowed_beside_a_pick_flag() {
    let expected = Invocation {
        action: Action::SelectLeft {
            path: picked("left"),
        },
        wait: true,
        new_window: true,
        read_only: true,
        profile: Some(PathBuf::from("p")),
        left_title: Some("L".to_owned()),
        right_title: Some("R".to_owned()),
    };
    let items = [
        "--select-left",
        "left",
        "--wait",
        "--new-window",
        "--read-only",
        "--profile",
        "p",
        "--left-title=L",
        "--right-title",
        "R",
    ];
    assert_eq!(parse_with(&items, &nothing), Ok(expected));
}

/// `--version` and `--help` still win over a pick flag.
#[test]
fn version_and_help_win_over_a_pick_flag() {
    assert_eq!(
        parse_with(&["--select-left", "left", "--version"], &nothing),
        Ok(bare(Action::Version))
    );
    assert_eq!(
        parse_with(&["--version", "--compare-with"], &nothing),
        Ok(bare(Action::Version))
    );
    assert_eq!(
        parse_with(&["--compare-with", "right", "--help"], &nothing),
        Ok(bare(Action::Help))
    );
}

/// An earlier scan error is reported before a pick and side conflict.
#[test]
fn an_earlier_scan_error_is_reported_first() {
    assert_eq!(
        parse_with(
            &[
                "--bogus",
                "--select-left",
                "left",
                "--compare-with",
                "right"
            ],
            &nothing
        ),
        Err(ArgError::UnknownFlag("--bogus".to_owned()))
    );
}

/// The combined pick refusal has the exact sentence from the contract.
#[test]
fn the_pick_sentence_is_exact() {
    assert_eq!(
        ArgError::PickWithSides.to_string(),
        "options `--select-left` and `--compare-with` take one path and no other side."
    );
}
