// Tests for IW6's Windows path rules: each refusal, the boundaries and a property check.

use std::ffi::OsStr;
use std::path::Path;

use proptest::prelude::*;

use super::{Candidate, FolderProblem, parse_candidate};

/// Parses one Windows path string.
fn parse(text: &str) -> Result<Candidate, FolderProblem> {
    parse_candidate(OsStr::new(text))
}

/// Parses a path the test expects to be refused and returns its problem.
fn refused(text: &str) -> FolderProblem {
    match parse(text) {
        Err(problem) => problem,
        Ok(candidate) => panic!("{text:?} was accepted as {:?}", candidate.path()),
    }
}

/// Parses a path the test expects to be accepted.
fn accepted(text: &str) -> Candidate {
    match parse(text) {
        Ok(candidate) => candidate,
        Err(problem) => panic!("{text:?} was refused with {problem:?}"),
    }
}

/// Relative and incomplete drive paths fail the absolute-path requirement.
#[test]
fn a_path_without_a_drive_letter_and_separator_is_refused() {
    for text in ["Navo", r"C:Navo", r"\Navo", "1:\\Navo", r"C|Navo", "C:"] {
        assert_eq!(refused(text), FolderProblem::NotAbsolute, "{text:?}");
    }
}

/// Navigation components cannot escape the selected folder.
#[test]
fn a_dot_or_dot_dot_component_is_refused() {
    for text in [r"C:\a\..\b", r"C:\a\.\b", r"C:\..\b", r"C:\.\b"] {
        assert_eq!(refused(text), FolderProblem::DotComponent, "{text:?}");
    }
}

/// Network and device prefixes cannot select an installation folder.
#[test]
fn unc_extended_and_device_paths_are_refused() {
    for text in [r"\\srv\s", r"\\?\C:\x", r"\\.\C:\x", "//srv/share"] {
        assert_eq!(
            refused(text),
            FolderProblem::NetworkOrDevicePath,
            "{text:?}"
        );
    }
}

/// Extra colons cannot introduce an alternate data stream.
#[test]
fn a_colon_after_the_drive_letter_is_refused() {
    for text in [r"C:\a:b", r"C:\a:b\Navo"] {
        assert_eq!(
            refused(text),
            FolderProblem::AlternateDataStream,
            "{text:?}"
        );
    }
}

/// Windows-invalid characters, including embedded NUL, are refused.
#[test]
fn characters_windows_rejects_are_refused() {
    for text in [
        r"C:\a<b",
        r"C:\a>b",
        r#"C:\a"b"#,
        r"C:\a|b",
        r"C:\a?b",
        r"C:\a*b",
        "C:\\a\u{1}b",
        "C:\\a\0b",
    ] {
        assert_eq!(refused(text), FolderProblem::InvalidCharacter, "{text:?}");
    }
}

/// Reserved device stems remain reserved inside a path or before an extension.
#[test]
fn a_reserved_device_name_is_refused() {
    for text in [
        r"CON",
        r"C:\CON\Navo",
        r"C:\nul",
        r"C:\COM1",
        r"C:\LPT9.txt",
        r"C:\conin$",
    ] {
        assert_eq!(refused(text), FolderProblem::ReservedName, "{text:?}");
    }
}

/// All six superscript device names are reserved with or without an extension.
#[test]
fn superscript_device_names_are_refused() {
    for name in ["COM¹", "COM²", "COM³", "LPT¹", "LPT²", "LPT³"] {
        for extension in ["", ".txt"] {
            let text = format!(r"C:\{name}{extension}");
            assert_eq!(refused(&text), FolderProblem::ReservedName, "{text:?}");
        }
    }
}

/// ASCII spaces at a device stem's end cannot hide its reserved name.
#[test]
fn space_padded_device_stems_are_refused() {
    assert_eq!(refused(r"C:\NUL .txt"), FolderProblem::ReservedName);
}

/// Windows normalization cannot discard a component's final dot or space.
#[test]
fn a_component_ending_in_a_dot_or_space_is_refused() {
    for text in [r"C:\a.", r"C:\a ", r"C:\a.\b", r"C:\a \b", r"C:\Navo."] {
        assert_eq!(refused(text), FolderProblem::TrailingDotOrSpace, "{text:?}");
    }
}

/// The limit counts UTF-16 units, including two units per supplementary symbol.
#[test]
fn a_path_longer_than_150_utf16_units_is_refused() {
    let accepted_150 = format!(r"C:\{}", "a".repeat(147));
    assert_eq!(accepted_150.encode_utf16().count(), 150);
    assert_eq!(accepted(&accepted_150).path(), Path::new(&accepted_150));

    let wide_150 = format!(r"C:\a{}", "😀".repeat(73));
    assert_eq!(wide_150.encode_utf16().count(), 150);
    assert_eq!(accepted(&wide_150).path(), Path::new(&wide_150));

    let refused_151 = format!(r"C:\{}", "a".repeat(148));
    assert_eq!(refused_151.encode_utf16().count(), 151);
    assert_eq!(refused(&refused_151), FolderProblem::TooLong);

    let wide_151 = format!(r"C:\{}", "😀".repeat(74));
    assert_eq!(wide_151.encode_utf16().count(), 151);
    assert_eq!(refused(&wide_151), FolderProblem::TooLong);
}

/// An installation must use a folder below the drive root.
#[test]
fn a_drive_root_is_refused() {
    for text in [r"C:\", "C:/", r"C:\\", r"D:\"] {
        assert_eq!(refused(text), FolderProblem::DriveRoot, "{text:?}");
    }
}

/// Each shell character has its exact character-specific refusal message.
#[test]
fn cmd_metacharacters_are_refused_and_named_in_the_message() {
    for (text, character) in [
        (r"C:\a%TEMP%b\Navo", '%'),
        (r"C:\R&D\Navo", '&'),
        (r"C:\a^b\Navo", '^'),
        (r"C:\Hi!\Navo", '!'),
    ] {
        let problem = refused(text);
        assert_eq!(
            problem,
            FolderProblem::ShellMetacharacter(character),
            "{text:?}"
        );
        assert_eq!(
            problem.message(),
            format!(
                "The folder name contains {character}, which Setup cannot pass safely to the uninstaller."
            )
        );
        assert!(!problem.fix().is_empty());
    }
}

/// Unicode names and internal spaces retain the user's spelling.
#[test]
fn non_ascii_and_internal_spaces_are_accepted() {
    for text in [r"C:\Prog Ă\NaVo", r"C:\a b\Navo", "C:\\€uro\\Navo"] {
        assert_eq!(accepted(text).path(), Path::new(text));
    }
}

/// Mixed, doubled and trailing separators are accepted without normalization.
#[test]
fn accepted_separator_forms_round_trip() {
    for text in [
        r"C:\Apps/Navo",
        r"C:\Apps\\Navo",
        "C:/Apps//Navo",
        r"C:\Apps\Navo\",
    ] {
        assert_eq!(accepted(text).path(), Path::new(text));
    }
}

/// Absolute-path advice requires a separator after the drive and colon.
#[test]
fn absolute_path_message_requires_a_separator() {
    assert_eq!(
        FolderProblem::NotAbsolute.message(),
        "The folder path must start with a drive letter, a colon and a separator, such as C:\\Navo."
    );
}

/// Length advice explains that supplementary symbols consume two units.
#[test]
fn too_long_message_explains_utf16_counting() {
    assert_eq!(
        FolderProblem::TooLong.message(),
        "The folder path is longer than 150 characters; some symbols count as two."
    );
}

/// Every refusal supplies both a reason and a recovery action.
#[test]
fn every_problem_has_a_message_and_a_fix() {
    let problems = [
        FolderProblem::NotAbsolute,
        FolderProblem::DotComponent,
        FolderProblem::NetworkOrDevicePath,
        FolderProblem::AlternateDataStream,
        FolderProblem::InvalidCharacter,
        FolderProblem::ReservedName,
        FolderProblem::TrailingDotOrSpace,
        FolderProblem::TooLong,
        FolderProblem::DriveRoot,
        FolderProblem::ShellMetacharacter('%'),
        FolderProblem::RootExists,
        FolderProblem::ReparsePoint,
        FolderProblem::ProtectedFolder,
        FolderProblem::DriveNotLocal,
        FolderProblem::SubstDrive,
        FolderProblem::ParentOpen,
        FolderProblem::NotWritable,
        FolderProblem::NotEnoughSpace,
        FolderProblem::ProbeFailed,
    ];
    for problem in problems {
        assert!(!problem.message().is_empty(), "{problem:?}");
        assert!(!problem.fix().is_empty(), "{problem:?}");
    }
}

proptest! {
    /// Arbitrary input never panics, and accepted input retains its spelling.
    #[test]
    fn parsing_never_panics_and_accepted_paths_round_trip(chars in prop::collection::vec(any::<char>(), 0..100)) {
        let text: String = chars.into_iter().collect();
        if let Ok(candidate) = parse(&text) {
            prop_assert_eq!(candidate.path(), Path::new(&text));
        }
    }

    /// Valid drive-prefixed Unicode paths are always accepted and round-trip.
    #[test]
    fn valid_drive_paths_are_accepted_and_round_trip(
        drive in prop::char::range('A', 'Z'),
        separator in prop_oneof![Just("\\"), Just("/")],
        parts in prop::collection::vec("Navo_[a-z0-9Ăé😀 ]{0,8}[a-z]", 1..5),
    ) {
        let text = format!("{drive}:{separator}{}", parts.join(separator));
        let candidate = parse(&text).unwrap();
        prop_assert_eq!(candidate.path(), Path::new(&text));
    }
}
