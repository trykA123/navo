//! Tests of the command-line parser: flags, limits, side specs and error messages.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::args::{Action, ArgError, Exit, Invocation, MAX_ARGS, MAX_BYTES, SideSpec, parse};

/// Turns string literals into the OsString list the parser takes.
fn to_os(items: &[&str]) -> Vec<OsString> {
    items.iter().map(OsString::from).collect()
}

/// Builds the default invocation around one action.
fn invocation(action: Action) -> Invocation {
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

/// Builds the start action with the given left side.
fn start(left: Option<SideSpec>) -> Action {
    Action::Start { left }
}

/// Builds a path side from the path text.
fn path(text: &str) -> SideSpec {
    SideSpec::Path(PathBuf::from(text))
}

/// Builds a Git side from the repository text and the revision.
fn git(repo: &str, rev: &str) -> SideSpec {
    SideSpec::GitRev {
        repo: PathBuf::from(repo),
        rev: rev.to_owned(),
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

/// Returns an argument that is not valid Unicode on this host.
#[cfg(unix)]
fn invalid_unicode() -> OsString {
    use std::os::unix::ffi::OsStringExt;
    OsString::from_vec(vec![0xff])
}

/// Returns an argument that is not valid Unicode on this host.
#[cfg(windows)]
fn invalid_unicode() -> OsString {
    use std::os::windows::ffi::OsStringExt;
    OsString::from_wide(&[0xD800])
}

/// Zero, one or two positionals start the screen or compare two joined sides.
#[test]
fn start_actions_cover_zero_to_two_sides() {
    assert_eq!(parse_with(&[], &nothing), Ok(invocation(start(None))));
    assert_eq!(
        parse_with(&["left"], &nothing),
        Ok(invocation(start(Some(path("cwd/left")))))
    );
    let pair = Action::Compare {
        left: path("cwd/left"),
        right: path("cwd/right"),
    };
    assert_eq!(
        parse_with(&["left", "right"], &nothing),
        Ok(invocation(pair))
    );
}

/// Three or four sides ask for three-way; `-o` and five sides are refused.
#[test]
fn three_way_forms_and_extra_positionals_are_refused() {
    for items in [
        vec!["base", "ours", "theirs"],
        vec!["base", "ours", "theirs", "merged"],
    ] {
        assert_eq!(parse_with(&items, &nothing), Err(ArgError::ThreeWay));
    }
    assert_eq!(
        parse_with(&["-o", "out"], &nothing),
        Err(ArgError::ThreeWay)
    );
    assert_eq!(
        parse_with(&["a", "b", "c", "d", "e"], &nothing),
        Err(ArgError::TooManyArgs(5))
    );
}

/// Every boolean flag sets its own field.
#[test]
fn boolean_flags_are_collected() {
    let expected = Invocation {
        wait: true,
        new_window: true,
        read_only: true,
        ..invocation(start(None))
    };
    assert_eq!(
        parse_with(&["--wait", "--new-window", "--read-only"], &nothing),
        Ok(expected)
    );
}

/// Value flags accept the next item and the `=` form, including titles.
#[test]
fn value_flags_accept_both_forms() {
    let expected = Invocation {
        profile: Some(PathBuf::from("dir")),
        left_title: Some("Left".to_owned()),
        right_title: Some("Right".to_owned()),
        ..invocation(start(None))
    };
    let items = [
        "--profile",
        "dir",
        "--left-title=Left",
        "--right-title",
        "Right",
    ];
    assert_eq!(parse_with(&items, &nothing), Ok(expected));
}

/// A value flag takes the next dash word, and a repeat keeps the last value.
#[test]
fn value_flags_take_next_item_and_repeats() {
    let dash = Invocation {
        profile: Some(PathBuf::from("-x")),
        ..invocation(start(None))
    };
    assert_eq!(parse_with(&["--profile", "-x"], &nothing), Ok(dash));
    let version = Invocation {
        profile: Some(PathBuf::from("--version")),
        ..invocation(start(None))
    };
    assert_eq!(
        parse_with(&["--profile", "--version"], &nothing),
        Ok(version)
    );
    let repeated = Invocation {
        profile: Some(PathBuf::from("b")),
        left_title: Some("B".to_owned()),
        ..invocation(start(None))
    };
    let items = [
        "--profile",
        "a",
        "--profile=b",
        "--left-title",
        "A",
        "--left-title=B",
    ];
    assert_eq!(parse_with(&items, &nothing), Ok(repeated));
}

/// A missing value and `=` on a boolean flag are refused, each with its item named.
#[test]
fn missing_or_equal_values_are_refused() {
    assert_eq!(
        parse_with(&["--profile"], &nothing),
        Err(ArgError::MissingValue("--profile"))
    );
    assert_eq!(
        parse_with(&["--left-title="], &nothing),
        Err(ArgError::MissingValue("--left-title"))
    );
    assert_eq!(
        parse_with(&["--wait=1"], &nothing),
        Err(ArgError::UnknownFlag("--wait=1".to_owned()))
    );
}

/// `--` ends flag handling; a lone dash and later dash words are positionals.
#[test]
fn double_dash_and_lone_dash_are_positionals() {
    assert_eq!(
        parse_with(&["--", "-file"], &nothing),
        Ok(invocation(start(Some(path("cwd/-file")))))
    );
    assert_eq!(
        parse_with(&["--", "--version"], &nothing),
        Ok(invocation(start(Some(path("cwd/--version")))))
    );
    assert_eq!(
        parse_with(&["-"], &nothing),
        Ok(invocation(start(Some(path("cwd/-")))))
    );
}

/// Version and help win over errors, positionals and each other.
#[test]
fn version_and_help_win() {
    assert_eq!(
        parse_with(&["--bogus", "--version"], &nothing),
        Ok(invocation(Action::Version))
    );
    assert_eq!(
        parse_with(&["--help", "--version"], &nothing),
        Ok(invocation(Action::Version))
    );
    assert_eq!(
        parse_with(&["a", "b", "c", "d", "e", "--version"], &nothing),
        Ok(invocation(Action::Version))
    );
    assert_eq!(
        parse_with(&["--help"], &nothing),
        Ok(invocation(Action::Help))
    );
    assert_eq!(
        parse_with(&["--bogus", "--help"], &nothing),
        Ok(invocation(Action::Help))
    );
}

/// Unknown long and short flags are reported with the whole item.
#[test]
fn unknown_flags_are_named() {
    assert_eq!(
        parse_with(&["--nope"], &nothing),
        Err(ArgError::UnknownFlag("--nope".to_owned()))
    );
    assert_eq!(
        parse_with(&["-x"], &nothing),
        Err(ArgError::UnknownFlag("-x".to_owned()))
    );
}

/// The count, byte and Unicode limits are refused with their size.
#[test]
fn argument_limits_are_refused() {
    let items = vec!["a"; MAX_ARGS + 1];
    assert_eq!(
        parse_with(&items, &nothing),
        Err(ArgError::TooManyArgs(MAX_ARGS + 1))
    );
    let long = "a".repeat(MAX_BYTES + 1);
    assert_eq!(
        parse_with(&[long.as_str()], &nothing),
        Err(ArgError::TooLong(MAX_BYTES + 1))
    );
    let got = parse(&[invalid_unicode()], Path::new("cwd"), &nothing);
    assert_eq!(got, Err(ArgError::NotUnicode));
}

/// The size limit runs before the Unicode check.
#[cfg(unix)]
#[test]
fn limits_run_before_unicode() {
    use std::os::unix::ffi::OsStringExt;
    let bad = OsString::from_vec(vec![0xff; MAX_BYTES + 1]);
    assert_eq!(
        parse(&[bad], Path::new("cwd"), &nothing),
        Err(ArgError::TooLong(MAX_BYTES + 1))
    );
}

/// An existing whole path with `@` stays one path; otherwise `repo@rev` splits.
#[test]
fn side_paths_win_and_git_sides_split() {
    let at_path = |file: &Path| file == Path::new("cwd/a@b");
    assert_eq!(
        parse_with(&["a@b"], &at_path),
        Ok(invocation(start(Some(path("cwd/a@b")))))
    );
    let repo = |file: &Path| file == Path::new("cwd/repo");
    assert_eq!(
        parse_with(&["repo@main"], &repo),
        Ok(invocation(start(Some(git("cwd/repo", "main")))))
    );
    assert_eq!(
        parse_with(&["repo@main@{1}"], &repo),
        Ok(invocation(start(Some(git("cwd/repo", "main@{1}")))))
    );
    let both = |file: &Path| file == Path::new("cwd/repo") || file == Path::new("cwd/repo@main");
    assert_eq!(
        parse_with(&["repo@main@tag"], &both),
        Ok(invocation(start(Some(git("cwd/repo@main", "tag")))))
    );
}

/// An empty prefix or a missing repository leaves the text as a plain path.
#[test]
fn empty_prefix_and_missing_repo_are_paths() {
    let repo = |file: &Path| file == Path::new("cwd/repo");
    assert_eq!(
        parse_with(&["@main"], &repo),
        Ok(invocation(start(Some(path("cwd/@main")))))
    );
    assert_eq!(
        parse_with(&["repo@main"], &nothing),
        Ok(invocation(start(Some(path("cwd/repo@main")))))
    );
}

/// Empty, dash-leading, control-bearing and over-long revisions are refused.
#[test]
fn bad_revisions_are_refused() {
    let repo = |file: &Path| file == Path::new("cwd/repo");
    for (item, rev) in [("repo@-x", "-x"), ("repo@", ""), ("repo@a\nb", "a\nb")] {
        assert_eq!(
            parse_with(&[item], &repo),
            Err(ArgError::BadRev(rev.to_owned()))
        );
    }
    let long_rev = "a".repeat(257);
    let long_item = format!("repo@{long_rev}");
    assert_eq!(
        parse_with(&[long_item.as_str()], &repo),
        Err(ArgError::BadRev(long_rev))
    );
}

/// Every error writes one plain sentence.
#[test]
fn error_messages_are_plain_sentences() {
    assert_eq!(
        ArgError::UnknownFlag("--z".to_owned()).to_string(),
        "unknown option \"--z\"."
    );
    assert_eq!(
        ArgError::MissingValue("--profile").to_string(),
        "option `--profile` needs a value."
    );
    assert_eq!(
        ArgError::NotUnicode.to_string(),
        "an argument is not valid Unicode."
    );
    assert_eq!(
        ArgError::BadRev("-x".to_owned()).to_string(),
        "the revision \"-x\" is not valid."
    );
    assert_eq!(
        ArgError::ThreeWay.to_string(),
        "three-way merge arrives in a later release."
    );
    assert_eq!(
        ArgError::TooManyArgs(257).to_string(),
        "too many arguments: 257."
    );
    assert_eq!(
        ArgError::TooLong(65537).to_string(),
        "the arguments are too long: 65537 bytes."
    );
}

/// The exit codes match NC10.
#[test]
fn exit_codes_match_nc10() {
    assert_eq!(Exit::Ok as i32, 0);
    assert_eq!(Exit::Discarded as i32, 1);
    assert_eq!(Exit::Error as i32, 2);
}

#[test]
fn error_text_escapes_control_characters() {
    let text = ArgError::BadRev("a\u{1b}[31m".to_owned()).to_string();
    assert!(!text.contains('\u{1b}'));
    assert_eq!(text, "the revision \"a\\u{1b}[31m\" is not valid.");
}
