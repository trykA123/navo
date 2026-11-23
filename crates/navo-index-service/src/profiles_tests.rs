//! Proves search hides other users' profiles through the service answer, keeps the totals of
//! the profiles root whole, hides other profiles' totals, and reads the scope from a fake
//! registry.

use std::sync::Arc;

use navo_index::file::Header;
use navo_index::ntfs::NtfsIndex;
use navo_index::pipe::{CancelToken, QueryTerm, Request, VolumeStatus};
use navo_index::table::{Flags, NO_PARENT, NameTable};
use navo_store::Search;

use super::scope_tests::Fake;
use super::*;
use crate::answer::{SharedVolume, State, answer};
use crate::search::SearchIndex;

const SID: &str = "S-1-5-21-1-2-3-1001";

/// A `C:` volume with `Users\{me,other,Public}`, one sized `.txt` file in each.
fn volume(scope: Option<ProfileScope>) -> SharedVolume {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let users = table.push(root, "Users", Flags::FOLDER).unwrap();
    for (folder, file) in [
        ("me", "a.txt"),
        ("other", "secret.txt"),
        ("Public", "b.txt"),
    ] {
        let folder = table.push(users, folder, Flags::FOLDER).unwrap();
        let file = table.push(folder, file, Flags::NONE).unwrap();
        table.set_size(file, 10).unwrap();
    }
    let frns = (0..u32::try_from(table.len()).unwrap()).map(|id| (u128::from(id) + 5, id));
    let header = Header {
        volume_serial: 1,
        journal_id: 2,
        next_usn: 10,
        root: "C:".into(),
        built_at: 0,
    };
    let index = NtfsIndex::restore((header, table, frns.collect()), 1, 2).unwrap();
    SharedVolume::new(State {
        index: Some(Arc::new(index)),
        status: VolumeStatus {
            root: "C:".into(),
            ..VolumeStatus::default()
        },
        search: SearchIndex::new(&Search::default()).with_profiles(scope),
        ..State::default()
    })
}

fn hit_paths(volume: &SharedVolume) -> Vec<String> {
    let request = Request {
        terms: vec![QueryTerm::Name("txt".into())],
    };
    let mut paths: Vec<_> = answer(std::slice::from_ref(volume), &request, &CancelToken::new())
        .hits
        .into_iter()
        .map(|hit| hit.path)
        .collect();
    paths.sort();
    paths
}

fn scope() -> ProfileScope {
    ProfileScope::new(r"c:\users\", r"C:\USERS\ME").unwrap()
}

#[test]
fn search_hides_other_profiles_but_returns_mine_and_public() {
    assert_eq!(
        hit_paths(&volume(Some(scope()))),
        [r"C:\Users\Public\b.txt", r"C:\Users\me\a.txt"]
    );
}

#[test]
fn search_without_a_scope_or_on_another_volume_hides_nothing() {
    let everything = [
        r"C:\Users\Public\b.txt",
        r"C:\Users\me\a.txt",
        r"C:\Users\other\secret.txt",
    ];
    assert_eq!(hit_paths(&volume(None)), everything);
    let elsewhere = ProfileScope::new(r"D:\Users", r"D:\Users\me").unwrap();
    assert_eq!(hit_paths(&volume(Some(elsewhere))), everything);
}

#[test]
fn search_with_a_missing_own_profile_still_hides_other_profiles() {
    let gone = ProfileScope::new(r"C:\Users", r"C:\Users\renamed").unwrap();
    assert_eq!(hit_paths(&volume(Some(gone))), [r"C:\Users\Public\b.txt"]);
}

#[test]
fn search_scope_keeps_root_totals_whole_and_hides_other_profiles_totals() {
    let volume = volume(Some(scope()));
    let request = Request {
        terms: vec![QueryTerm::FolderTotalsV1(vec![
            r"C:\Users".into(),
            r"C:\Users\other".into(),
            r"C:\Users\missing".into(),
            r"C:\Users\me".into(),
        ])],
    };
    let totals = answer(std::slice::from_ref(&volume), &request, &CancelToken::new())
        .totals
        .unwrap()
        .totals;
    let counts: Vec<_> = totals
        .iter()
        .map(|total| total.as_ref().map(|total| (total.bytes, total.files)))
        .collect();
    assert_eq!(counts, [Some((30, 3)), None, None, Some((10, 1))]);
}

#[test]
fn search_scope_reads_the_registry_and_names_each_missing_part() {
    let mut fake = Fake::new(r"C:\Users", &[(SID, r"C:\Users\me")]);
    assert_eq!(
        scope_from(SID, &fake),
        ProfileScope::new(r"C:\Users", r"C:\Users\me")
    );
    fake.profile(SID, r"\\server\home\me");
    assert!(matches!(
        scope_from(SID, &fake),
        Err(ProfileUnresolved::NotLocal(_))
    ));
    fake.forget(SID);
    assert_eq!(
        scope_from(SID, &fake),
        Err(ProfileUnresolved::MissingProfile)
    );
    let empty = Fake::new("", &[]);
    assert_eq!(scope_from(SID, &empty), Err(ProfileUnresolved::MissingRoot));
}

#[test]
fn search_scope_rejects_paths_that_are_not_folders_on_a_drive() {
    for path in [
        r"C:\",
        "C:",
        r"\\?\C:\Users",
        r"C:\Users\..\x",
        r"C:\a\\b",
        "Users",
    ] {
        assert!(
            ProfileScope::new(path, r"C:\Users\me").is_err(),
            "{path} must not resolve"
        );
    }
}

#[test]
fn search_scope_hides_only_direct_children_of_guarded_folders() {
    let ids = ProfileIds::Guarded {
        guarded: vec![1],
        kept: vec![2],
        hidden: vec![7],
    };
    assert!(ids.hides(3, Some(1)));
    assert!(!ids.hides(2, Some(1)));
    assert!(!ids.hides(1, Some(0)));
    assert!(!ids.hides(4, Some(3)));
    assert!(ids.hides(7, Some(6)));
    assert!(ProfileIds::Closed.hides(0, None));
    assert!(!ProfileIds::Open.hides(3, Some(1)));
}
