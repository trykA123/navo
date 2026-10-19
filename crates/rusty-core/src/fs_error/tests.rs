use super::*;
use crate::ProviderPath;
use FsErrorKind::*;
use RecoveryAction::*;

const ALL_KINDS: [FsErrorKind; 11] = [
    NotFound,
    AccessDenied,
    SharingViolation,
    Locked,
    DiskFull,
    PathTooLong,
    NameInvalid,
    NetworkLost,
    Timeout,
    Cancelled,
    Unknown,
];

const WINDOWS_CODES: [(u32, FsErrorKind); 27] = [
    (2, NotFound),
    (3, NotFound),
    (5, AccessDenied),
    (32, SharingViolation),
    (33, Locked),
    (39, DiskFull),
    (112, DiskFull),
    (111, PathTooLong),
    (206, PathTooLong),
    (123, NameInvalid),
    (267, NameInvalid),
    (53, NetworkLost),
    (64, NetworkLost),
    (67, NetworkLost),
    (1231, NetworkLost),
    (55, NetworkLost),
    (59, NetworkLost),
    (1232, NetworkLost),
    (1236, NetworkLost),
    (2250, NetworkLost),
    (121, Timeout),
    (1460, Timeout),
    (1223, Cancelled),
    (87, Unknown),
    (0, Unknown),
    (u32::MAX, Unknown),
    (1, Unknown),
];

const ERRNO_CODES: [(i32, FsErrorKind); 21] = [
    (2, NotFound),
    (1, AccessDenied),
    (13, AccessDenied),
    (26, SharingViolation),
    (16, Locked),
    (28, DiskFull),
    (36, PathTooLong),
    (22, NameInvalid),
    (84, NameInvalid),
    (20, NameInvalid),
    (101, NetworkLost),
    (103, NetworkLost),
    (104, NetworkLost),
    (112, NetworkLost),
    (113, NetworkLost),
    (110, Timeout),
    (125, Cancelled),
    (5, Unknown),
    (0, Unknown),
    (-1, Unknown),
    (i32::MAX, Unknown),
];

fn error(kind: FsErrorKind, location: Option<&str>) -> FsError {
    FsError {
        location: location.map(|text| Location::new(text).unwrap()),
        ..FsError::new(kind)
    }
}

#[test]
fn windows_codes_map_to_their_kind() {
    let got: Vec<(u32, FsErrorKind)> = WINDOWS_CODES
        .iter()
        .map(|&(code, _)| (code, FsErrorKind::from_windows_code(code)))
        .collect();
    assert_eq!(got, WINDOWS_CODES);
}

#[test]
fn errno_values_map_to_their_kind() {
    let got: Vec<(i32, FsErrorKind)> = ERRNO_CODES
        .iter()
        .map(|&(code, _)| (code, FsErrorKind::from_errno(code)))
        .collect();
    assert_eq!(got, ERRNO_CODES);
}

#[test]
fn every_kind_has_its_recovery_actions() {
    let expected: [(FsErrorKind, &[RecoveryAction]); 11] = [
        (NotFound, &[Skip, SkipAll, Cancel]),
        (AccessDenied, &[RetryAsAdmin, Skip, SkipAll, Cancel]),
        (SharingViolation, &[Retry, Skip, SkipAll, Cancel]),
        (Locked, &[Retry, Skip, SkipAll, Cancel]),
        (DiskFull, &[Retry, Cancel]),
        (PathTooLong, &[Rename, Skip, Cancel]),
        (NameInvalid, &[Rename, Skip, Cancel]),
        (NetworkLost, &[Reconnect, Retry, Cancel]),
        (Timeout, &[Retry, Cancel]),
        (Cancelled, &[]),
        (Unknown, &[Retry, Skip, Cancel]),
    ];
    let got: Vec<(FsErrorKind, &[RecoveryAction])> = ALL_KINDS
        .iter()
        .map(|&kind| (kind, error(kind, None).recovery()))
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn recovery_does_not_depend_on_location_or_code() {
    let differing: Vec<FsErrorKind> = ALL_KINDS
        .into_iter()
        .filter(|&kind| {
            let full = FsError {
                os_code: Some(5),
                ..error(kind, Some(r"C:\Data\report.xlsx"))
            };
            full.recovery() != error(kind, None).recovery()
        })
        .collect();
    assert_eq!(differing, []);
}

#[test]
fn messages_of_every_kind() {
    let lines: Vec<String> = ALL_KINDS
        .iter()
        .flat_map(|&kind| {
            [None, Some(r"C:\Data\report.xlsx")]
                .map(|location| format!("{kind:?}: {}", error(kind, location).message()))
        })
        .collect();
    insta::with_settings!({ prepend_module_to_snapshot => false }, {
        insta::assert_snapshot!("fs_error_messages", lines.join("\n"));
    });
}

#[test]
fn message_names_the_location_with_display() {
    let path = ProviderPath::from_wide(&[0x43, 0x3A, 0x5C, 0x61, 0xD800]);
    let failure = FsError {
        location: Some(Location::local(path).unwrap()),
        ..error(NotFound, None)
    };
    assert!(failure.message().contains("C:\\a\u{FFFD}"));
}

#[test]
fn message_never_holds_the_os_text() {
    let leaking: Vec<String> = ERRNO_CODES
        .iter()
        .map(|&(code, kind)| {
            let failure = FsError {
                os_code: Some(code),
                ..error(kind, Some(r"C:\Data\report.xlsx"))
            };
            let os_text = std::io::Error::from_raw_os_error(code).to_string();
            let description = os_text
                .split(" (os error")
                .next()
                .unwrap_or_default()
                .to_owned();
            (failure.message(), description)
        })
        .filter(|(message, description)| {
            message.contains(description.as_str()) || message.contains("os error")
        })
        .map(|(message, _)| message)
        .collect();
    assert_eq!(leaking, Vec::<String>::new());
}

fn assert_holder_message(names: &[&str], programs: &str) {
    for kind in [SharingViolation, Locked] {
        for (location, subject) in [(None, "The item"), (Some("report.docx"), "“report.docx”")]
        {
            let failure = FsError {
                holders: (1u32..)
                    .zip(names)
                    .map(|(pid, &name)| LockHolder {
                        name: name.to_owned(),
                        pid,
                    })
                    .collect(),
                ..error(kind, location)
            };
            assert_eq!(
                failure.message(),
                format!("{subject} is in use by {programs}."),
                "{kind:?} at {location:?}"
            );
            assert_eq!(failure.recovery(), error(kind, location).recovery());
        }
    }
}

#[test]
fn lock_message_names_one_holder() {
    assert_holder_message(&["Microsoft Word"], "Microsoft Word");
}

#[test]
fn lock_message_names_two_holders() {
    assert_holder_message(&["Microsoft Word", "Excel"], "Microsoft Word and Excel");
}

#[test]
fn lock_message_counts_three_or_more_holders() {
    assert_holder_message(
        &["Microsoft Word", "Excel", "Notepad"],
        "Microsoft Word, Excel and 1 more program",
    );
    assert_holder_message(
        &["Microsoft Word", "Excel", "Notepad", "Explorer"],
        "Microsoft Word, Excel and 2 more programs",
    );
}

#[test]
fn lock_message_lists_duplicate_names_once_in_first_seen_order() {
    assert_holder_message(&["Word", "Word"], "Word");
    assert_holder_message(&["Word", "Excel", "Word", "Excel"], "Word and Excel");
    assert_holder_message(
        &["Word", "Excel", "Word", "Notepad", "Excel", "Explorer"],
        "Word, Excel and 2 more programs",
    );
}

#[test]
fn lock_message_without_holders_keeps_the_original_text() {
    for (kind, suffix) in [
        (SharingViolation, "is in use by another program."),
        (Locked, "is locked."),
    ] {
        assert_eq!(error(kind, None).message(), format!("The item {suffix}"));
        assert_eq!(
            error(kind, Some("report.docx")).message(),
            format!("“report.docx” {suffix}")
        );
    }
}
