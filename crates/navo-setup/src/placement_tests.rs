// Tests for IW7 placement: chain shape, refused folders, drive facts and the space rule.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use super::{
    Chain, DriveKind, Facts, FileId, Link, NameEq, Protected, Refused, Space, browse_target, check,
    inside, same_path,
};
use crate::syntax::FolderProblem;

/// ASCII-only name comparison for the pure tests; W7a supplies the Windows one.
struct AsciiNames;

impl NameEq for AsciiNames {
    fn same(&self, a: &OsStr, b: &OsStr) -> bool {
        a.to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
    }
}

/// A stable file id for the test volume.
fn id(value: u128) -> FileId {
    FileId {
        volume: 7,
        id: value,
    }
}

/// One existing folder in the chain.
fn link(name: &str, value: u128) -> Link {
    Link {
        path: PathBuf::from(format!(r"C:\{name}")),
        name: OsString::from(name),
        id: id(value),
        reparse: false,
    }
}

/// A chain with the given existing links and missing names.
fn chain(existing: Vec<Link>, missing: &[&str]) -> Chain {
    Chain {
        existing,
        missing: missing.iter().map(|name| OsString::from(*name)).collect(),
    }
}

/// Facts for a writable fixed drive with ample space.
fn valid_facts() -> Facts {
    Facts {
        drive: DriveKind::Fixed,
        subst: false,
        writable: true,
        free: 1_000_000_000,
    }
}

/// A protected folder anchored at `anchor`.
fn protected(anchor: u128, tail: &[&str], case_sensitive: Option<bool>) -> Protected {
    Protected {
        anchor: id(anchor),
        anchor_case_sensitive: case_sensitive,
        tail: tail.iter().map(|name| OsString::from(*name)).collect(),
    }
}

/// Calls `check` with the ASCII fake and no Setup.exe requirement.
fn check_default(
    chain: &Chain,
    refused: &Refused,
    facts: &Facts,
    payload: u64,
) -> Result<(), FolderProblem> {
    check(
        chain,
        refused,
        facts,
        &Space {
            payload,
            compressed: 0,
            extracted: 0,
        },
        &AsciiNames,
    )
}

/// The three existing ancestors every placement test starts from.
fn existing() -> Vec<Link> {
    vec![link("C:", 1), link("Users", 2), link("a", 3)]
}

#[test]
fn a_new_folder_on_a_local_drive_is_accepted() {
    let chain = chain(existing(), &["Navo"]);
    assert_eq!(
        check_default(&chain, &Refused::default(), &valid_facts(), 100),
        Ok(())
    );
}

#[test]
fn a_reparse_at_each_depth_is_refused() {
    for depth in 0..3 {
        let mut existing = existing();
        existing[depth].reparse = true;
        let chain = chain(existing, &["Navo"]);
        assert_eq!(
            check_default(&chain, &Refused::default(), &valid_facts(), 100),
            Err(FolderProblem::ReparsePoint),
            "depth {depth}"
        );
    }
}

#[test]
fn an_identity_match_ignores_the_spelling_of_paths_and_names() {
    let chain = chain(
        vec![link("C:", 1), link("USERS", 2), link("A", 3)],
        &["DESKTOP", "Navo"],
    );
    let folder = protected(3, &["Desktop"], Some(false));
    assert!(inside(&chain, &folder, &AsciiNames));
}

#[test]
fn a_never_inside_folder_refuses_itself_and_children() {
    let mut refused = Refused::default();
    refused
        .never_inside
        .push(protected(3, &["Desktop"], Some(false)));
    for missing in [vec!["Desktop"], vec!["Desktop", "Navo"]] {
        let chain = chain(existing(), &missing);
        assert_eq!(
            check_default(&chain, &refused, &valid_facts(), 100),
            Err(FolderProblem::ProtectedFolder),
            "missing {missing:?}"
        );
    }
}

#[test]
fn a_never_equal_folder_refuses_itself_but_allows_children() {
    let mut refused = Refused::default();
    refused
        .never_equal
        .push(protected(3, &["Desktop"], Some(false)));
    let equal = chain(existing(), &["Desktop"]);
    assert_eq!(
        check_default(&equal, &refused, &valid_facts(), 100),
        Err(FolderProblem::ProtectedFolder)
    );
    let child = chain(existing(), &["Desktop", "Navo"]);
    assert_eq!(check_default(&child, &refused, &valid_facts(), 100), Ok(()));
}

#[test]
fn an_existing_root_is_refused() {
    let chain = chain(existing(), &[]);
    assert_eq!(
        check_default(&chain, &Refused::default(), &valid_facts(), 100),
        Err(FolderProblem::RootExists)
    );
}

#[test]
fn a_remote_or_unknown_drive_is_refused() {
    for drive in [DriveKind::Remote, DriveKind::Other] {
        let mut facts = valid_facts();
        facts.drive = drive;
        let chain = chain(existing(), &["Navo"]);
        assert_eq!(
            check_default(&chain, &Refused::default(), &facts, 100),
            Err(FolderProblem::DriveNotLocal),
            "{drive:?}"
        );
    }
}

#[test]
fn a_removable_drive_is_accepted() {
    let mut facts = valid_facts();
    facts.drive = DriveKind::Removable;
    let chain = chain(existing(), &["Navo"]);
    assert_eq!(
        check_default(&chain, &Refused::default(), &facts, 100),
        Ok(())
    );
}

#[test]
fn a_subst_drive_is_refused() {
    let mut facts = valid_facts();
    facts.subst = true;
    let chain = chain(existing(), &["Navo"]);
    assert_eq!(
        check_default(&chain, &Refused::default(), &facts, 100),
        Err(FolderProblem::SubstDrive)
    );
}

#[test]
fn a_folder_setup_cannot_write_is_refused() {
    let mut facts = valid_facts();
    facts.writable = false;
    let chain = chain(existing(), &["Navo"]);
    assert_eq!(
        check_default(&chain, &Refused::default(), &facts, 100),
        Err(FolderProblem::NotWritable)
    );
}

#[test]
fn low_space_is_refused_and_four_times_the_payload_is_enough() {
    let mut facts = valid_facts();
    facts.free = 79_999_999;
    let chain = chain(existing(), &["Navo"]);
    assert_eq!(
        check_default(&chain, &Refused::default(), &facts, 20_000_000),
        Err(FolderProblem::NotEnoughSpace)
    );
    facts.free = 80_000_000;
    assert_eq!(
        check_default(&chain, &Refused::default(), &facts, 20_000_000),
        Ok(())
    );
}

#[test]
fn velopack_requirement_adds_compressed_extracted_and_fifty_mb() {
    let chain = chain(existing(), &["Navo"]);
    let space = Space {
        payload: 1,
        compressed: 10,
        extracted: 20,
    };
    let facts = Facts {
        free: 50_000_029,
        ..valid_facts()
    };
    assert_eq!(
        check(&chain, &Refused::default(), &facts, &space, &AsciiNames),
        Err(FolderProblem::NotEnoughSpace)
    );
    let facts = Facts {
        free: 50_000_030,
        ..valid_facts()
    };
    assert_eq!(
        check(&chain, &Refused::default(), &facts, &space, &AsciiNames),
        Ok(())
    );
}

#[test]
fn a_case_sensitive_or_unknown_anchor_refuses_every_candidate_under_it() {
    for case_sensitive in [Some(true), None] {
        let mut refused = Refused::default();
        refused
            .never_inside
            .push(protected(3, &["Navo"], case_sensitive));
        let chain = chain(existing(), &["Other"]);
        assert_eq!(
            check_default(&chain, &refused, &valid_facts(), 100),
            Err(FolderProblem::ProtectedFolder),
            "{case_sensitive:?}"
        );
    }
}

#[test]
fn browse_returns_a_navo_folder_inside_the_picked_parent() {
    assert_eq!(
        browse_target(Path::new(r"C:\Users\a")),
        PathBuf::from(r"C:\Users\a\Navo")
    );
    assert_eq!(browse_target(Path::new(r"C:\")), PathBuf::from(r"C:\Navo"));
    assert_eq!(
        browse_target(Path::new("C:/Users/a")),
        PathBuf::from(r"C:\Users\a\Navo")
    );
}

#[derive(Clone, Copy)]
enum Profile {
    Local,
    Roaming,
}

/// The clean-profile ancestor chain under `%LocalAppData%` or `%AppData%`.
fn profile_chain(profile: Profile, extra: Option<&str>, missing: &[&str]) -> Chain {
    let mut existing = vec![
        link("C:", 1),
        link("Users", 2),
        link("a", 3),
        link("AppData", 4),
    ];
    match profile {
        Profile::Local => existing.push(link("Local", 5)),
        Profile::Roaming => existing.push(link("Roaming", 6)),
    }
    if let Some(extra) = extra {
        existing.push(link(extra, 7));
    }
    chain(existing, missing)
}

/// The IW8a folders for a profile where Navo's own folders do not exist yet.
fn clean_refused() -> Refused {
    Refused {
        never_inside: vec![
            protected(5, &["Navo"], Some(false)),
            protected(6, &["Navo"], Some(false)),
        ],
        never_equal: Vec::new(),
        fresh_only: vec![protected(5, &["NavoApp"], Some(false))],
    }
}

#[test]
fn a_clean_profile_protects_navo_and_the_legacy_root() {
    let refused = clean_refused();
    for chain in [
        profile_chain(Profile::Local, None, &["Navo"]),
        profile_chain(Profile::Local, None, &["NAVO", "x", "Navo"]),
        profile_chain(Profile::Roaming, None, &["Navo"]),
        profile_chain(Profile::Local, None, &["NavoApp"]),
    ] {
        assert_eq!(
            check_default(&chain, &refused, &valid_facts(), 100),
            Err(FolderProblem::ProtectedFolder),
            "{:?}",
            chain.missing
        );
    }
    for chain in [
        profile_chain(Profile::Local, Some("Programs"), &["Navo"]),
        profile_chain(Profile::Local, Some("NavoTools"), &["Navo"]),
    ] {
        assert_eq!(
            check_default(&chain, &refused, &valid_facts(), 100),
            Ok(()),
            "{:?}",
            chain.missing
        );
    }
}

#[test]
fn unknown_or_missing_chain_data_refuses() {
    let empty = Chain {
        existing: Vec::new(),
        missing: vec![OsString::from("Navo")],
    };
    assert_eq!(
        check_default(&empty, &Refused::default(), &valid_facts(), 100),
        Err(FolderProblem::ProbeFailed)
    );
    let blank = chain(existing(), &[""]);
    assert_eq!(
        check_default(&blank, &Refused::default(), &valid_facts(), 100),
        Err(FolderProblem::ProbeFailed)
    );
}

/// Component-wise path equality follows the injected case rule.
#[test]
fn same_path_compares_components_with_the_name_rule() {
    let names = AsciiNames;
    #[cfg(windows)]
    let installed = Path::new("C:/Users/U/AppData/Local/Programs/Navo");
    #[cfg(not(windows))]
    let installed = Path::new("/Users/U/AppData/Local/Programs/Navo");
    #[cfg(windows)]
    let lower = Path::new("c:/users/u/appdata/local/programs/navo");
    #[cfg(not(windows))]
    let lower = Path::new("/users/u/appdata/local/programs/navo");
    #[cfg(windows)]
    let sibling = Path::new("C:/Users/U/AppData/Local/Programs/NavoOld");
    #[cfg(not(windows))]
    let sibling = Path::new("/Users/U/AppData/Local/Programs/NavoOld");
    #[cfg(windows)]
    let parent = Path::new("C:/Users/U/../U/AppData/Local/Programs/Navo");
    #[cfg(not(windows))]
    let parent = Path::new("/Users/U/../U/AppData/Local/Programs/Navo");
    assert!(same_path(lower, installed, &names));
    assert!(!same_path(sibling, installed, &names));
    assert!(!same_path(Path::new("Navo"), installed, &names));
    assert!(!same_path(parent, installed, &names));
}
