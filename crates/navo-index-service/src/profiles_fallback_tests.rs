//! Proves the fallback scope fails closed: an unresolved own profile still hides every other
//! folder under the profiles root (or `\Users` on the system drive), keeps `Public` and a
//! derivable own profile, and logs one line saying other users' folders are hidden.

use super::super::scope_tests::{Fake, hits, totals, volume};
use super::*;
use crate::service_log::{LOG_NAME, Log};

const SID: &str = "S-1-5-21-1-2-3-1001";
const OTHER: &str = "S-1-5-21-1-2-3-1002";

/// Files under `Users` for the own profile, another user, `Public` and `Default`, plus one
/// outside `Users`.
const FILES: [&str; 5] = [
    r"Users\me\a.txt",
    r"Users\other\secret.txt",
    r"Users\Public\b.txt",
    r"Users\Default\d.txt",
    r"Windows\w.txt",
];

#[test]
fn fallback_hides_the_root_children_but_public_when_the_own_profile_is_unresolved() {
    let fake = Fake::new(r"C:\Users", &[(OTHER, r"C:\Users\other")]);
    let c = volume("C:", &FILES, Some(fallback_scope(SID, &fake, Some("C:"))));
    assert_eq!(hits(&c), [r"C:\Users\Public\b.txt", r"C:\Windows\w.txt"]);
    assert_eq!(totals(&c, &[r"C:\Users\other"]), [None]);
}

#[test]
fn fallback_hides_the_root_children_when_the_own_profile_is_a_network_home() {
    let fake = Fake::new(r"C:\Users", &[(SID, r"\\server\homes\me")]);
    let c = volume("C:", &FILES, Some(fallback_scope(SID, &fake, Some("C:"))));
    assert_eq!(hits(&c), [r"C:\Users\Public\b.txt", r"C:\Windows\w.txt"]);
}

#[test]
fn fallback_keeps_a_derivable_own_profile_visible() {
    let fake = Fake::new("", &[(SID, r"C:\Users\me")]);
    let c = volume("C:", &FILES, Some(fallback_scope(SID, &fake, Some("C:"))));
    assert_eq!(
        hits(&c),
        [
            r"C:\Users\Public\b.txt",
            r"C:\Users\me\a.txt",
            r"C:\Windows\w.txt"
        ]
    );
}

#[test]
fn fallback_guards_users_on_the_system_drive_when_the_root_is_unknown() {
    let fake = Fake::new("", &[]);
    let scope = fallback_scope(SID, &fake, Some(r"D:\"));
    let d = volume("D:", &FILES, Some(scope.clone()));
    assert_eq!(hits(&d), [r"D:\Users\Public\b.txt", r"D:\Windows\w.txt"]);
    let c = volume("C:", &FILES, Some(scope));
    assert_eq!(hits(&c).len(), FILES.len(), "other drives keep their Users");
}

#[test]
fn fallback_guards_users_on_every_drive_when_the_system_drive_is_unknown() {
    let fake = Fake::new("", &[]);
    let scope = fallback_scope(SID, &fake, None);
    for root in ["C:", "E:"] {
        let drive = volume(root, &FILES, Some(scope.clone()));
        assert_eq!(
            hits(&drive),
            [
                format!(r"{root}\Users\Public\b.txt"),
                format!(r"{root}\Windows\w.txt")
            ]
        );
    }
}

#[test]
fn unresolved_profiles_log_that_other_users_folders_are_hidden() {
    let folder = tempfile::tempdir().unwrap();
    let fake = Fake::new("", &[]);
    let scope = scope_or_fallback(SID, &fake, Some("C:"), &Log::open(folder.path()));
    assert_eq!(scope, fallback_scope(SID, &fake, Some("C:")));
    let log = std::fs::read_to_string(folder.path().join(LOG_NAME)).unwrap();
    assert!(
        log.contains(
            "profiles unresolved: the profiles root is missing from the registry; \
             other users' folders are hidden"
        ),
        "{log}"
    );
}

#[test]
fn resolved_profiles_keep_the_registry_scope_and_log_nothing() {
    let folder = tempfile::tempdir().unwrap();
    let fake = Fake::new(
        r"C:\Users",
        &[(SID, r"C:\Users\me"), (OTHER, r"C:\Else\other")],
    );
    let scope = scope_or_fallback(SID, &fake, Some("C:"), &Log::open(folder.path()));
    assert_eq!(scope, scope_from(SID, &fake).unwrap());
    let log = std::fs::read_to_string(folder.path().join(LOG_NAME)).unwrap_or_default();
    assert!(!log.contains("profiles unresolved"), "{log}");
}
