// Tests for Atom 0: Program Files refuses with NeedsAdministrator, before the writable check.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use super::{
    Chain, DriveKind, Facts, FileId, Link, NameEq, Protected, Refused, Space, check,
    refused_problem,
};
use crate::syntax::FolderProblem;

/// ASCII-only name comparison for the pure tests; the Windows probe supplies the real one.
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

/// The ancestors of a folder under Program Files (x86), which keeps its id under any spelling.
fn program_files(spelling: &str) -> Vec<Link> {
    vec![link("C:", 1), link(spelling, 2)]
}

/// Facts for a fixed local drive with ample space; `writable` is the fact these tests vary.
fn facts(writable: bool) -> Facts {
    Facts {
        drive: DriveKind::Fixed,
        subst: false,
        parent_open: false,
        writable,
        free: 1_000_000_000,
    }
}

/// A protected folder anchored at `anchor`, compared case-insensitively below it.
fn protected(anchor: u128, tail: &[&str]) -> Protected {
    Protected {
        anchor: id(anchor),
        anchor_case_sensitive: Some(false),
        tail: tail.iter().map(|name| OsString::from(*name)).collect(),
    }
}

/// The Atom 0 refused sets: Program Files (x86) is admin-only and still never-inside.
fn refused() -> Refused {
    Refused {
        admin_only: vec![protected(2, &[])],
        never_inside: vec![protected(2, &[])],
        ..Refused::default()
    }
}

/// Calls `check` with the ASCII fake and a 100-byte payload.
fn check_with(chain: &Chain, refused: &Refused, facts: &Facts) -> Result<(), FolderProblem> {
    check(
        chain,
        refused,
        facts,
        &Space {
            payload: 100,
            compressed: 0,
            extracted: 0,
        },
        &AsciiNames,
    )
}

#[test]
fn an_unwritable_folder_below_program_files_x86_gives_needs_administrator() {
    let chain = chain(program_files("Program Files (x86)"), &["Navo"]);
    assert_eq!(
        check_with(&chain, &refused(), &facts(false)),
        Err(FolderProblem::NeedsAdministrator)
    );
}

#[test]
fn a_writable_folder_below_program_files_x86_gives_needs_administrator() {
    let chain = chain(program_files("Program Files (x86)"), &["Navo"]);
    assert_eq!(
        check_with(&chain, &refused(), &facts(true)),
        Err(FolderProblem::NeedsAdministrator)
    );
}

#[test]
fn a_short_name_for_program_files_x86_gives_needs_administrator() {
    for spelling in ["PROGRA~2", "program files (x86)"] {
        let chain = chain(program_files(spelling), &["NAVO", "Tools", "Navo"]);
        assert_eq!(
            check_with(&chain, &refused(), &facts(false)),
            Err(FolderProblem::NeedsAdministrator),
            "{spelling}"
        );
    }
}

#[test]
fn a_never_inside_folder_that_cannot_be_written_gives_protected_folder() {
    let refused = Refused {
        never_inside: vec![protected(2, &[])],
        ..Refused::default()
    };
    let chain = chain(vec![link("C:", 1), link("Windows", 2)], &["Navo"]);
    assert_eq!(
        check_with(&chain, &refused, &facts(false)),
        Err(FolderProblem::ProtectedFolder)
    );
}

#[test]
fn an_unwritable_plain_folder_still_gives_not_writable() {
    let chain = chain(vec![link("C:", 1), link("Tools", 2)], &["Navo"]);
    assert_eq!(
        check_with(&chain, &Refused::default(), &facts(false)),
        Err(FolderProblem::NotWritable)
    );
}

#[test]
fn needs_administrator_wins_over_never_inside_and_over_low_space() {
    let mut facts = facts(false);
    facts.free = 0;
    let chain = chain(program_files("Program Files (x86)"), &["Navo"]);
    assert_eq!(
        check_with(&chain, &refused(), &facts),
        Err(FolderProblem::NeedsAdministrator)
    );
}

#[test]
fn the_earlier_refusals_still_win_over_needs_administrator() {
    let candidate = chain(program_files("Program Files (x86)"), &["Navo"]);
    assert_eq!(
        check_with(&candidate, &refused(), &facts(false)),
        Err(FolderProblem::NeedsAdministrator)
    );
    let mut reparse = candidate.clone();
    reparse.existing[1].reparse = true;
    assert_eq!(
        check_with(&reparse, &refused(), &facts(false)),
        Err(FolderProblem::ReparsePoint)
    );
    let root = chain(program_files("Program Files (x86)"), &[]);
    assert_eq!(
        check_with(&root, &refused(), &facts(false)),
        Err(FolderProblem::RootExists)
    );
    let mut subst = facts(false);
    subst.subst = true;
    assert_eq!(
        check_with(&candidate, &refused(), &subst),
        Err(FolderProblem::SubstDrive)
    );
    let mut remote = facts(false);
    remote.drive = DriveKind::Remote;
    assert_eq!(
        check_with(&candidate, &refused(), &remote),
        Err(FolderProblem::DriveNotLocal)
    );
}

#[test]
fn needs_administrator_says_what_is_wrong_and_what_to_choose() {
    assert_eq!(
        FolderProblem::NeedsAdministrator.message(),
        "Program Files needs administrator rights, and this version of Setup cannot ask for them yet."
    );
    assert_eq!(
        FolderProblem::NeedsAdministrator.fix(),
        r"Choose a folder such as C:\Navo, or keep the default folder."
    );
}

#[test]
fn refused_problem_is_none_for_a_folder_outside_every_group() {
    let chain = chain(vec![link("C:", 1), link("Tools", 9)], &["Navo"]);
    assert_eq!(refused_problem(&chain, &refused(), &AsciiNames), None);
    assert_eq!(
        refused_problem(&chain, &Refused::default(), &AsciiNames),
        None
    );
}

#[test]
fn refused_problem_reads_admin_only_before_the_other_groups() {
    let chain = chain(program_files("Program Files (x86)"), &["Navo"]);
    assert_eq!(
        refused_problem(&chain, &refused(), &AsciiNames),
        Some(FolderProblem::NeedsAdministrator)
    );
    let only_never_inside = Refused {
        never_inside: vec![protected(2, &[])],
        ..Refused::default()
    };
    assert_eq!(
        refused_problem(&chain, &only_never_inside, &AsciiNames),
        Some(FolderProblem::ProtectedFolder)
    );
}

#[test]
fn refused_problem_keeps_the_old_groups_rules() {
    let refused = Refused {
        never_equal: vec![protected(2, &["Tools"])],
        fresh_only: vec![protected(2, &["NavoApp"])],
        ..Refused::default()
    };
    let equal = chain(program_files("Program Files"), &["Tools"]);
    assert_eq!(
        refused_problem(&equal, &refused, &AsciiNames),
        Some(FolderProblem::ProtectedFolder)
    );
    let child = chain(program_files("Program Files"), &["Tools", "Navo"]);
    assert_eq!(refused_problem(&child, &refused, &AsciiNames), None);
    let fresh = chain(program_files("Program Files"), &["NavoApp", "Navo"]);
    assert_eq!(
        refused_problem(&fresh, &refused, &AsciiNames),
        Some(FolderProblem::ProtectedFolder)
    );
}

#[test]
fn an_admin_only_anchor_with_unknown_case_refuses_every_candidate_below_it() {
    for case_sensitive in [Some(true), None] {
        let refused = Refused {
            admin_only: vec![Protected {
                anchor_case_sensitive: case_sensitive,
                ..protected(2, &["Other"])
            }],
            ..Refused::default()
        };
        let chain = chain(program_files("Program Files (x86)"), &["Navo"]);
        assert_eq!(
            refused_problem(&chain, &refused, &AsciiNames),
            Some(FolderProblem::NeedsAdministrator),
            "{case_sensitive:?}"
        );
    }
}
