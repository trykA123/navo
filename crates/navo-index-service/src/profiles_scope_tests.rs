//! Proves the profile scope follows junctions, hides every other profile wherever it lives,
//! keeps a nested own profile reachable, and logs one line when a lookup timeout hides hits.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use navo_index::file::Header;
use navo_index::ntfs::NtfsIndex;
use navo_index::pipe::{CancelToken, QueryTerm, Request, VolumeStatus};
use navo_index::table::{Flags, NO_PARENT, NameTable};
use navo_store::Search;

use super::*;
use crate::answer::{SharedVolume, State, answer};
use crate::search::SearchIndex;

const SID: &str = "S-1-5-21-1-2-3-1001";
const OTHER: &str = "S-1-5-21-1-2-3-1002";

/// A fake registry and file system: the profiles root, one profile path per SID, and the
/// final path behind each junction.
pub(super) struct Fake {
    root: Option<String>,
    profiles: HashMap<String, String>,
    finals: HashMap<String, String>,
    list_error: Option<String>,
}

impl Fake {
    /// A registry with `root` (empty for none) and `(sid, path)` profiles, without junctions.
    pub(super) fn new(root: &str, profiles: &[(&str, &str)]) -> Self {
        Self {
            root: (!root.is_empty()).then(|| root.to_owned()),
            profiles: profiles
                .iter()
                .map(|(sid, path)| ((*sid).to_owned(), (*path).to_owned()))
                .collect(),
            finals: HashMap::new(),
            list_error: None,
        }
    }

    pub(super) fn profile(&mut self, sid: &str, path: &str) {
        self.profiles.insert(sid.to_owned(), path.to_owned());
    }

    pub(super) fn forget(&mut self, sid: &str) {
        self.profiles.remove(sid);
    }

    fn junction(mut self, path: &str, target: &str) -> Self {
        self.finals.insert(path.to_lowercase(), target.to_owned());
        self
    }
}

impl ProfileSource for Fake {
    fn text(&self, key: &str, value: &str) -> Option<String> {
        if key == PROFILE_LIST && value == "ProfilesDirectory" {
            return self.root.clone();
        }
        let sid = key.strip_prefix(PROFILE_LIST)?.strip_prefix('\\')?;
        (value == "ProfileImagePath")
            .then(|| self.profiles.get(sid).cloned())
            .flatten()
    }

    fn subkeys(&self, key: &str) -> Result<Vec<String>, String> {
        assert_eq!(key, PROFILE_LIST, "only the profile list is enumerated");
        match &self.list_error {
            Some(reason) => Err(reason.clone()),
            None => Ok(self.profiles.keys().cloned().collect()),
        }
    }

    fn final_path(&self, path: &str) -> Option<String> {
        let target = self.finals.get(&path.to_lowercase())?;
        Some(format!(r"\\?\{target}"))
    }
}

/// A volume `root` holding one sized file at each path in `files`, folders made on the way.
fn volume(root: &str, files: &[&str], scope: Option<ProfileScope>) -> SharedVolume {
    let mut table = NameTable::new();
    let top = table.push(NO_PARENT, root, Flags::FOLDER).unwrap();
    let mut folders = HashMap::new();
    for file in files {
        let parts: Vec<&str> = file.split('\\').collect();
        let mut parent = top;
        for depth in 1..parts.len() {
            let key = parts[..depth].join("\\");
            parent = *folders
                .entry(key)
                .or_insert_with(|| table.push(parent, parts[depth - 1], Flags::FOLDER).unwrap());
        }
        let id = table
            .push(parent, parts[parts.len() - 1], Flags::NONE)
            .unwrap();
        table.set_size(id, 10).unwrap();
    }
    let frns = (0..u32::try_from(table.len()).unwrap()).map(|id| (u128::from(id) + 5, id));
    let header = Header {
        volume_serial: 1,
        journal_id: 2,
        next_usn: 10,
        root: root.into(),
        built_at: 0,
    };
    let index = NtfsIndex::restore((header, table, frns.collect()), 1, 2).unwrap();
    SharedVolume::new(State {
        index: Some(Arc::new(index)),
        status: VolumeStatus {
            root: root.into(),
            ..VolumeStatus::default()
        },
        search: SearchIndex::new(&Search::default()).with_profiles(scope),
        ..State::default()
    })
}

/// Every `.txt` hit path, sorted, through the real service answer.
fn hits(volume: &SharedVolume) -> Vec<String> {
    let request = Request {
        terms: vec![QueryTerm::Name("txt".into())],
    };
    let reply = answer(std::slice::from_ref(volume), &request, &CancelToken::new());
    let mut paths: Vec<_> = reply.hits.into_iter().map(|hit| hit.path).collect();
    paths.sort();
    paths
}

/// Folder totals `(bytes, files)` per path, through the real service answer.
fn totals(volume: &SharedVolume, paths: &[&str]) -> Vec<Option<(u64, u32)>> {
    let request = Request {
        terms: vec![QueryTerm::FolderTotalsV1(
            paths.iter().map(|path| (*path).to_owned()).collect(),
        )],
    };
    let reply = answer(std::slice::from_ref(volume), &request, &CancelToken::new());
    let totals = reply.totals.unwrap().totals;
    totals
        .iter()
        .map(|total| total.as_ref().map(|total| (total.bytes, total.files)))
        .collect()
}

#[test]
fn search_scope_follows_a_junctioned_profiles_root_to_its_final_volume() {
    let fake = Fake::new(
        r"C:\Users",
        &[(SID, r"C:\Users\me"), (OTHER, r"C:\Users\other")],
    )
    .junction(r"C:\Users", r"D:\Users")
    .junction(r"C:\Users\me", r"D:\Users\me")
    .junction(r"C:\Users\other", r"D:\Users\other");
    let scope = scope_from(SID, &fake).unwrap();
    let files = [
        r"Users\me\a.txt",
        r"Users\other\secret.txt",
        r"Users\Public\b.txt",
    ];
    let moved = volume("D:", &files, Some(scope));
    assert_eq!(
        hits(&moved),
        [r"D:\Users\Public\b.txt", r"D:\Users\me\a.txt"]
    );
    assert_eq!(totals(&moved, &[r"D:\Users\other"]), [None]);
}

#[test]
fn search_scope_hides_other_profiles_outside_the_profiles_root() {
    let fake = Fake::new(
        r"C:\Users",
        &[
            (SID, r"C:\Users\me"),
            (OTHER, r"%SystemDrive%\x"),
            ("S-1-5-18", r"C:\Profiles\svc"),
        ],
    )
    .junction(r"C:\Profiles\svc", r"E:\Moved\svc");
    let scope = scope_from(SID, &fake).unwrap();
    let files = [r"Users\me\a.txt", r"Profiles\svc\secret.txt", r"Data\c.txt"];
    let c = volume("C:", &files, Some(scope.clone()));
    assert_eq!(hits(&c), [r"C:\Data\c.txt", r"C:\Users\me\a.txt"]);
    let e = volume(
        "E:",
        &[r"Moved\svc\secret.txt", r"Moved\keep.txt"],
        Some(scope),
    );
    assert_eq!(hits(&e), [r"E:\Moved\keep.txt"]);
}

#[test]
fn search_scope_keeps_the_path_to_a_nested_own_profile_and_hides_its_siblings() {
    let fake = Fake::new(r"C:\Users", &[(SID, r"C:\Users\CORP\me")]);
    let scope = scope_from(SID, &fake).unwrap();
    let files = [
        r"Users\CORP\me\a.txt",
        r"Users\CORP\colleague\secret.txt",
        r"Users\CORP\note.txt",
        r"Users\other\secret.txt",
        r"Users\Public\b.txt",
    ];
    let c = volume("C:", &files, Some(scope));
    assert_eq!(
        hits(&c),
        [r"C:\Users\CORP\me\a.txt", r"C:\Users\Public\b.txt"]
    );
    let answered = totals(
        &c,
        &[r"C:\Users", r"C:\Users\CORP\colleague", r"C:\Users\CORP\me"],
    );
    assert_eq!(answered, [Some((50, 5)), None, Some((10, 1))]);
}

#[test]
fn search_scope_hides_totals_below_a_hidden_profile_like_a_missing_path() {
    let fake = Fake::new(
        r"C:\Users",
        &[(SID, r"C:\Users\me"), (OTHER, r"C:\Users\other")],
    );
    let scope = scope_from(SID, &fake).unwrap();
    let files = [r"Users\me\a.txt", r"Users\other\deep\secret.txt"];
    let c = volume("C:", &files, Some(scope));
    let answered = totals(
        &c,
        &[r"C:\", r"C:\Users\other\deep", r"C:\Users\nobody\deep"],
    );
    assert_eq!(answered, [Some((20, 2)), None, None]);
}

#[test]
fn search_scope_drops_an_other_profile_that_holds_the_own_one() {
    let fake = Fake::new(r"C:\Users", &[(SID, r"C:\Users\me"), (OTHER, r"C:\Users")]);
    let scope = scope_from(SID, &fake).unwrap();
    let c = volume("C:", &[r"Users\me\a.txt"], Some(scope));
    assert_eq!(hits(&c), [r"C:\Users\me\a.txt"]);
}

#[test]
fn search_scope_reports_an_unreadable_profile_list_and_still_guards_the_root() {
    let mut fake = Fake::new(r"C:\Users", &[(SID, r"C:\Users\me"), (OTHER, r"C:\Else\x")]);
    fake.list_error = Some("access denied".into());
    let scope = scope_from(SID, &fake).unwrap();
    assert_eq!(scope.others_unread(), Some("access denied"));
    let c = volume("C:", &[r"Users\me\a.txt", r"Users\x\s.txt"], Some(scope));
    assert_eq!(hits(&c), [r"C:\Users\me\a.txt"]);
}

#[test]
fn search_scope_timeout_closes_the_volume_and_logs_once() {
    let folder = tempfile::tempdir().unwrap();
    let notice = Arc::new(TimeoutNotice::new(Log::open(folder.path())));
    let scope = ProfileScope::new(r"C:\Users", r"C:\Users\me").unwrap();
    let index = SearchIndex::new(&Search::default())
        .with_profiles(Some(scope))
        .with_timeout_notice(Arc::clone(&notice));
    let table = NameTable::new();
    for _ in 0..2 {
        assert_eq!(
            index.profile_ids(&table, Instant::now()),
            ProfileIds::Closed
        );
    }
    let log = std::fs::read_to_string(folder.path().join(crate::service_log::LOG_NAME)).unwrap();
    assert_eq!(log.matches("profile lookup timed out").count(), 1, "{log}");
}

#[test]
fn search_scope_guards_an_other_profile_that_contains_the_own_one() {
    let fake = Fake::new(
        r"C:\Users",
        &[(SID, r"D:\Home\CORP\me"), (OTHER, r"D:\Home\CORP")],
    );
    let scope = scope_from(SID, &fake).unwrap();
    let files = [r"Home\CORP\me\a.txt", r"Home\CORP\colleague\x.txt"];
    let d = volume("D:", &files, Some(scope));
    assert_eq!(hits(&d), [r"D:\Home\CORP\me\a.txt"]);
}

#[test]
fn search_scope_both_forms_strips_the_long_prefix_and_pins_edge_targets() {
    let fake = Fake::new(r"C:\Users", &[])
        .junction(r"C:\Users\unc", r"UNC\srv\share\x")
        .junction(r"C:\Users\slash", r"D:\Users\slash\")
        .junction(r"C:\Users\case", r"c:\USERS\Case");
    let cases: [(&str, &[&str]); 3] = [
        (r"C:\Users\unc", &[r"C:\Users\unc"]),
        (r"C:\Users\slash", &[r"C:\Users\slash", r"D:\Users\slash"]),
        (r"C:\Users\case", &[r"C:\Users\case"]),
    ];
    for (path, expected) in cases {
        assert_eq!(both_forms(&fake, path).unwrap(), expected, "{path}");
    }
}
