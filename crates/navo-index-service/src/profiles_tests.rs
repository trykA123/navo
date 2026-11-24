//! Proves search hides other users' profiles through the service answer, keeps the totals of
//! the profiles root whole, hides other profiles' totals, and reads the scope from a fake
//! registry.

use std::sync::Arc;

use navo_index::file::Header;
use navo_index::ntfs::NtfsIndex;
use navo_index::pipe::{CancelToken, QueryTerm, Request, VolumeStatus};
use navo_index::table::{Flags, NO_PARENT, NameTable};
use navo_index::usn::{UsnBatch, UsnRecord};
use navo_store::Search;

use super::scope_tests::Fake;
use super::*;
use crate::answer::{SharedVolume, State, answer};
use crate::search::SearchIndex;

const SID: &str = "S-1-5-21-1-2-3-1001";

/// A `C:` volume with `Users\{me,other,Public}`, one sized `.txt` file in each.
fn volume(scope: Option<ProfileScope>) -> SharedVolume {
    SharedVolume::new(State {
        index: Some(Arc::new(profile_index(&[]))),
        status: VolumeStatus {
            root: "C:".into(),
            ..VolumeStatus::default()
        },
        search: SearchIndex::new(&Search::default()).with_profiles(scope),
        ..State::default()
    })
}

/// The `volume` index, with the `first` folders pushed under `C:` before `Users`, so every
/// later id shifts; file references are each id plus 5.
fn profile_index(first: &[&str]) -> NtfsIndex {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    for name in first {
        table.push(root, name, Flags::FOLDER).unwrap();
    }
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
    NtfsIndex::restore((header, table, frns.collect()), 1, 2).unwrap()
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

/// Asks `volume` for the totals of `paths`.
fn totals(volume: &SharedVolume, paths: &[&str]) -> Vec<Option<navo_index::pipe::FolderTotals>> {
    let request = Request {
        terms: vec![QueryTerm::FolderTotalsV1(
            paths.iter().map(|path| (*path).into()).collect(),
        )],
    };
    answer(std::slice::from_ref(volume), &request, &CancelToken::new())
        .totals
        .unwrap()
        .totals
}

/// One journal record for `file_id` under `parent_id`; a name without a dot is a folder, and
/// only a file that is not deleted reports a size.
fn record(file_id: u128, parent_id: u128, reason: u32, name: &str) -> UsnRecord {
    let folder = !name.contains('.');
    UsnRecord {
        file_id,
        parent_id,
        usn: 10,
        reason,
        flags: if folder { Flags::FOLDER } else { Flags::NONE },
        name: name.into(),
        size: (!folder && reason != DELETE).then_some(10),
    }
}

/// Applies `records` to `volume` as one journal batch ending at `next_usn`.
fn apply(volume: &SharedVolume, next_usn: i64, records: Vec<UsnRecord>) {
    let batch = UsnBatch { next_usn, records };
    let unresolved = crate::batch::apply_batch(&volume.shared, batch).unwrap();
    assert!(unresolved.is_empty(), "{unresolved:?}");
}

const RENAME_OLD: u32 = 0x1000;
const RENAME_NEW: u32 = 0x2000;
const CREATE: u32 = 0x100;
const DELETE: u32 = 0x200;
/// File references of `Users` and `me` in `profile_index(&[])`.
const USERS: u128 = 6;
const ME: u128 = 7;

#[test]
fn two_searches_and_a_totals_request_reuse_one_profile_resolution() {
    let volume = volume(Some(scope()));
    let mine = [r"C:\Users\Public\b.txt", r"C:\Users\me\a.txt"];
    assert_eq!(hit_paths(&volume), mine);
    assert_eq!(hit_paths(&volume), mine);
    let other = totals(&volume, &[r"C:\Users\other", r"C:\Users\me"]);
    assert!(other[0].is_none() && other[1].is_some(), "{other:?}");
    let resolutions = volume.shared.read().unwrap().search.profile_resolutions();
    assert_eq!(
        resolutions, 1,
        "an unchanged table must reuse its resolved ids"
    );
}

#[test]
fn a_batch_that_renames_or_deletes_a_profile_folder_resolves_the_scope_again() {
    let volume = volume(Some(scope()));
    assert_eq!(
        hit_paths(&volume),
        [r"C:\Users\Public\b.txt", r"C:\Users\me\a.txt"]
    );
    apply(
        &volume,
        20,
        vec![
            record(ME, USERS, RENAME_OLD, "me"),
            record(ME, USERS, RENAME_NEW, "renamed"),
        ],
    );
    assert_eq!(
        hit_paths(&volume),
        [r"C:\Users\Public\b.txt"],
        "renamed own profile"
    );
    assert!(totals(&volume, &[r"C:\Users\renamed"])[0].is_none());
    apply(
        &volume,
        30,
        vec![
            record(50, USERS, CREATE, "me"),
            record(51, 50, CREATE, "c.txt"),
        ],
    );
    assert_eq!(
        hit_paths(&volume),
        [r"C:\Users\Public\b.txt", r"C:\Users\me\c.txt"]
    );
    apply(
        &volume,
        40,
        vec![
            record(51, 50, DELETE, "c.txt"),
            record(50, USERS, DELETE, "me"),
            record(ME, USERS, RENAME_OLD, "renamed"),
            record(ME, USERS, RENAME_NEW, "me"),
        ],
    );
    assert_eq!(
        hit_paths(&volume),
        [r"C:\Users\Public\b.txt", r"C:\Users\me\a.txt"]
    );
    assert!(totals(&volume, &[r"C:\Users\me"])[0].is_some());
}

#[test]
fn a_replaced_index_resolves_the_scope_again() {
    let volume = volume(Some(scope()));
    assert_eq!(
        hit_paths(&volume),
        [r"C:\Users\Public\b.txt", r"C:\Users\me\a.txt"]
    );
    crate::rebuild_session::replace(&volume.shared, profile_index(&["Extra"]));
    assert_eq!(
        hit_paths(&volume),
        [r"C:\Users\Public\b.txt", r"C:\Users\me\a.txt"]
    );
    assert!(totals(&volume, &[r"C:\Users\other"])[0].is_none());
}

#[test]
fn a_different_table_never_reuses_the_cached_ids() {
    let search = SearchIndex::new(&Search::default()).with_profiles(Some(scope()));
    let far = Instant::now() + std::time::Duration::from_secs(600);
    let (first, shifted) = (profile_index(&[]), profile_index(&["Extra"]));
    let expected = |index: &NtfsIndex| {
        ProfileIds::resolve_with(Some(&scope()), &mut index.table.folder_lookup(), far)
    };
    assert_ne!(expected(&first), expected(&shifted));
    for index in [&first, &shifted, &first] {
        assert_eq!(*search.profile_ids(&index.table, far), expected(index));
    }
}

/// A `C:` table whose `Windows\System32` holds `wide` files besides `config\systemprofile`.
fn system32_table(wide: u32) -> NameTable {
    let mut table = NameTable::new();
    let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    let users = table.push(root, "Users", Flags::FOLDER).unwrap();
    table.push(users, "me", Flags::FOLDER).unwrap();
    let windows = table.push(root, "Windows", Flags::FOLDER).unwrap();
    let system32 = table.push(windows, "System32", Flags::FOLDER).unwrap();
    let config = table.push(system32, "config", Flags::FOLDER).unwrap();
    table.push(config, "systemprofile", Flags::FOLDER).unwrap();
    for number in 0..wide {
        let flags = if number % 10 == 0 {
            Flags::FOLDER
        } else {
            Flags::NONE
        };
        table
            .push(system32, &format!("entry{number}.dll"), flags)
            .unwrap();
    }
    table
}

#[test]
#[ignore = "measurement: run alone with --nocapture"]
fn measure_profile_resolution_cost() {
    let table = system32_table(10_000);
    let system = r"C:\Windows\System32\config\systemprofile".to_owned();
    let scope = ProfileScope::build(
        &[r"C:\Users".into()],
        &[r"C:\Users\me".into()],
        vec![system],
    );
    let index = SearchIndex::new(&Search::default()).with_profiles(Some(scope.clone()));
    let far = Instant::now() + std::time::Duration::from_secs(600);
    let rounds = 200;
    let started = Instant::now();
    for _ in 0..rounds {
        let ids = ProfileIds::resolve_with(Some(&scope), &mut table.folder_lookup(), far);
        assert!(matches!(ids, ProfileIds::Guarded { ref hidden, .. } if hidden.len() == 1));
    }
    let cold = started.elapsed() / rounds;
    let started = Instant::now();
    for _ in 0..rounds {
        let _ = index.profile_ids(&table, far);
    }
    let per_request = started.elapsed() / rounds;
    eprintln!("IXH3 cost: cold resolve {cold:?}, per request {per_request:?}");
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
