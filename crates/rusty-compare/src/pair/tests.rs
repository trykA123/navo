use std::cmp::Ordering;

use proptest::prelude::*;
use rusty_core::ProviderPath;

use super::*;

const NAMES: [&str; 12] = [
    "A", "a", "B", "b", "README", "readme", "ß", "SS", "Σ", "σ", "x", "y",
];

fn rel(text: &str) -> RelPath {
    RelPath::new(ProviderPath::from_text(text))
}

fn wide_rel(units: &[u16]) -> RelPath {
    RelPath::new(ProviderPath::from_wide(units))
}

fn entry(rel: RelPath, kind: EntryKind) -> SideEntry {
    SideEntry::new(rel, kind, 0, None)
}

fn file(name: &str) -> SideEntry {
    entry(rel(name), EntryKind::File)
}

fn folder(name: &str) -> SideEntry {
    entry(rel(name), EntryKind::Folder)
}

fn text_of(entry: &SideEntry) -> String {
    entry.rel.as_path().to_text()
}

fn names(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            row.left
                .as_deref()
                .or(row.right.as_deref())
                .map_or_else(String::new, text_of)
        })
        .collect()
}

#[test]
fn empty_lists_pair_to_no_rows() {
    assert!(pair(Vec::new(), Vec::new(), CaseRule::Sensitive).is_empty());
}

#[test]
fn entries_without_a_counterpart_are_one_sided() {
    let left = file("only-left.txt");
    let right = file("only-right.txt");

    let rows = pair(vec![left.clone()], vec![right.clone()], CaseRule::Sensitive);

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].raw_status, Status::OnlyLeft);
    assert_eq!(rows[0].display_status, Status::OnlyLeft);
    assert_eq!(rows[0].left.as_deref(), Some(&left));
    assert!(rows[0].right.is_none());
    assert_eq!(rows[0].generation, Generation::default());
    assert_eq!(rows[0].content, ContentClass::Unknown);

    assert_eq!(rows[1].raw_status, Status::OnlyRight);
    assert_eq!(rows[1].display_status, Status::OnlyRight);
    assert!(rows[1].left.is_none());
    assert_eq!(rows[1].right.as_deref(), Some(&right));
}

#[test]
fn a_matching_name_pairs_both_entries() {
    let left = SideEntry::new(rel("report.txt"), EntryKind::File, 12, Some(1));
    let right = SideEntry::new(rel("report.txt"), EntryKind::File, 34, Some(2));

    let rows = pair(vec![left.clone()], vec![right.clone()], CaseRule::Sensitive);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].raw_status, Status::Checking);
    assert_eq!(rows[0].display_status, Status::Checking);
    assert_eq!(rows[0].left.as_deref(), Some(&left));
    assert_eq!(rows[0].right.as_deref(), Some(&right));
}

#[test]
fn a_file_against_a_folder_is_a_type_conflict() {
    let left = file("item");
    let right = folder("item");

    let rows = pair(vec![left.clone()], vec![right.clone()], CaseRule::Sensitive);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].raw_status, Status::TypeConflict);
    assert_eq!(rows[0].display_status, Status::TypeConflict);
    assert_eq!(rows[0].left.as_deref(), Some(&left));
    assert_eq!(rows[0].right.as_deref(), Some(&right));
}

#[test]
fn folders_come_first_and_names_use_ordinal_order() {
    let left = vec![folder("zeta"), file("beta.txt")];
    let right = vec![folder("zeta"), file("Alpha.txt"), file("alpha.txt")];

    let rows = pair(left, right, CaseRule::Sensitive);

    assert_eq!(names(&rows), ["zeta", "Alpha.txt", "alpha.txt", "beta.txt"]);
    assert_eq!(rows[0].raw_status, Status::Checking);
    assert_eq!(rows[1].raw_status, Status::OnlyRight);
    assert_eq!(rows[2].raw_status, Status::OnlyRight);
    assert_eq!(rows[3].raw_status, Status::OnlyLeft);
}

#[test]
fn non_bmp_names_sort_by_codepoint_order() {
    let rows = pair(
        vec![file("\u{10000}")],
        vec![file("\u{E000}")],
        CaseRule::Sensitive,
    );

    assert_eq!(names(&rows), ["\u{E000}", "\u{10000}"]);
}

#[test]
fn case_sensitive_pairing_keeps_case_variants_apart() {
    let rows = pair(
        vec![file("A.txt")],
        vec![file("a.txt")],
        CaseRule::Sensitive,
    );

    assert_eq!(names(&rows), ["A.txt", "a.txt"]);
    assert_eq!(rows[0].raw_status, Status::OnlyLeft);
    assert_eq!(rows[1].raw_status, Status::OnlyRight);
}

#[test]
fn case_insensitive_pairing_matches_names_that_differ_only_by_case() {
    let left = file("Report.TXT");
    let right = file("report.txt");

    let rows = pair(
        vec![left.clone()],
        vec![right.clone()],
        CaseRule::Insensitive,
    );

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].raw_status, Status::Checking);
    assert_eq!(rows[0].left.as_deref(), Some(&left));
    assert_eq!(rows[0].right.as_deref(), Some(&right));
}

#[test]
fn the_case_rule_is_insensitive_when_either_side_is() {
    assert_eq!(
        CaseRule::for_sides(true, true),
        CaseRule::Sensitive,
        "both case-sensitive sides can pair exactly"
    );
    assert_eq!(CaseRule::for_sides(false, true), CaseRule::Insensitive);
    assert_eq!(CaseRule::for_sides(true, false), CaseRule::Insensitive);
    assert_eq!(CaseRule::for_sides(false, false), CaseRule::Insensitive);
}

#[test]
fn case_variants_on_the_sensitive_side_get_case_conflicts() {
    let left = file("readme");
    let right_lower = file("readme");
    let right_upper = file("README");

    let rows = pair(
        vec![left.clone()],
        vec![right_upper.clone(), right_lower.clone()],
        CaseRule::Insensitive,
    );

    assert_eq!(rows.len(), 2);
    assert_eq!(names(&rows), ["README", "readme"]);
    assert!(
        rows.iter()
            .all(|row| row.raw_status == Status::CaseConflict)
    );
    assert!(
        rows.iter()
            .all(|row| row.display_status == Status::CaseConflict)
    );

    assert!(rows[0].left.is_none());
    assert_eq!(rows[0].right.as_deref(), Some(&right_upper));
    assert_eq!(rows[1].left.as_deref(), Some(&left));
    assert_eq!(rows[1].right.as_deref(), Some(&right_lower));
}

#[test]
fn case_variants_without_a_counterpart_are_still_case_conflicts() {
    let rows = pair(
        vec![file("A"), file("a")],
        Vec::new(),
        CaseRule::Insensitive,
    );

    assert_eq!(names(&rows), ["A", "a"]);
    assert!(
        rows.iter()
            .all(|row| row.raw_status == Status::CaseConflict)
    );
    assert!(rows.iter().all(|row| row.right.is_none()));
}

#[test]
fn unicode_case_folds_follow_windows_rules() {
    let greek = pair(
        vec![file("Σίσυφος")],
        vec![file("ΣΊΣΥΦΟΣ")],
        CaseRule::Insensitive,
    );
    assert_eq!(greek.len(), 1);
    assert_eq!(greek[0].raw_status, Status::Checking);

    let sharp_s = pair(
        vec![file("straße")],
        vec![file("STRASSE")],
        CaseRule::Insensitive,
    );
    assert_eq!(
        sharp_s.iter().map(|row| row.raw_status).collect::<Vec<_>>(),
        [Status::OnlyRight, Status::OnlyLeft]
    );

    let supplementary = pair(
        vec![file("\u{10428}")],
        vec![file("\u{10400}")],
        CaseRule::Insensitive,
    );
    assert_eq!(
        supplementary
            .iter()
            .map(|row| row.raw_status)
            .collect::<Vec<_>>(),
        [Status::OnlyRight, Status::OnlyLeft]
    );
}

#[test]
fn unpaired_surrogates_keep_their_identity() {
    let surrogate = wide_rel(&[0xD800]);
    let replacement = rel("\u{FFFD}");
    let left = SideEntry::new(surrogate.clone(), EntryKind::File, 0, None);

    let rows = pair(
        vec![left.clone()],
        vec![SideEntry::new(replacement, EntryKind::File, 0, None)],
        CaseRule::Insensitive,
    );

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].raw_status, Status::OnlyRight);
    assert!(rows[0].left.is_none());
    assert_eq!(rows[1].left.as_deref(), Some(&left));
    assert_eq!(rows[1].raw_status, Status::OnlyLeft);

    let identical = pair(
        vec![SideEntry::new(surrogate.clone(), EntryKind::File, 0, None)],
        vec![SideEntry::new(surrogate, EntryKind::File, 0, None)],
        CaseRule::Insensitive,
    );
    assert_eq!(identical.len(), 1);
    assert_eq!(identical[0].raw_status, Status::Checking);
}

#[test]
fn canonical_order_collisions_use_raw_units() {
    let surrogate = wide_rel(&[0xD800]);
    let real = wide_rel(&[0xDBC0, 0xDC00]);
    assert_eq!(
        compare_canonical(surrogate.as_path(), real.as_path()),
        Ordering::Equal
    );

    let entries = vec![
        SideEntry::new(surrogate.clone(), EntryKind::File, 0, None),
        SideEntry::new(real.clone(), EntryKind::File, 0, None),
    ];
    let expected = vec![surrogate, real];

    for _ in 0..20 {
        let rows = pair(entries.clone(), Vec::new(), CaseRule::Sensitive);
        let order = rows
            .iter()
            .map(|row| row.left.as_ref().map(|entry| entry.rel.clone()))
            .collect::<Option<Vec<_>>>()
            .unwrap();
        assert_eq!(order, expected);
    }
}

fn any_kind() -> impl Strategy<Value = EntryKind> {
    prop_oneof![
        Just(EntryKind::File),
        Just(EntryKind::Folder),
        Just(EntryKind::Link),
        Just(EntryKind::Submodule),
    ]
}

fn any_name() -> impl Strategy<Value = RelPath> {
    prop_oneof![
        (0..NAMES.len()).prop_map(|index| rel(NAMES[index])),
        prop::collection::vec(any::<u16>(), 1..5)
            .prop_map(|units| RelPath::new(ProviderPath::from_wide(&units))),
    ]
}

fn any_entries(base: u64) -> impl Strategy<Value = Vec<SideEntry>> {
    prop::collection::vec((any_name(), any_kind(), any::<bool>()), 0..8).prop_map(move |items| {
        let mut entries: Vec<SideEntry> = items
            .into_iter()
            .enumerate()
            .map(|(index, (rel, kind, ignored))| {
                let tag = base + u64::try_from(index).unwrap();
                let mut entry = SideEntry::new(rel, kind, tag, None);
                entry.ignored = ignored;
                entry
            })
            .collect();
        entries.sort_by(|a, b| compare_paths(a.rel.as_path(), b.rel.as_path()));
        entries
    })
}

proptest! {
    #[test]
    fn every_input_entry_lands_in_exactly_one_row(
        left in any_entries(0),
        right in any_entries(1_000_000),
        insensitive in any::<bool>(),
    ) {
        let case = if insensitive {
            CaseRule::Insensitive
        } else {
            CaseRule::Sensitive
        };
        let mut expected_left: Vec<u64> = left.iter().map(|entry| entry.size).collect();
        let mut expected_right: Vec<u64> = right.iter().map(|entry| entry.size).collect();
        expected_left.sort_unstable();
        expected_right.sort_unstable();

        let rows = pair(left, right, case);
        let mut seen_left = Vec::new();
        let mut seen_right = Vec::new();
        for row in &rows {
            prop_assert!(row.left.is_some() || row.right.is_some());
            if let (Some(left), Some(right)) = (&row.left, &row.right) {
                match case {
                    CaseRule::Sensitive => prop_assert_eq!(&left.rel, &right.rel),
                    CaseRule::Insensitive => prop_assert!(left
                        .rel
                        .as_path()
                        .windows_eq(right.rel.as_path())),
                }
            }
            if let Some(entry) = &row.left {
                seen_left.push(entry.size);
            }
            if let Some(entry) = &row.right {
                seen_right.push(entry.size);
            }
        }
        seen_left.sort_unstable();
        seen_right.sort_unstable();
        prop_assert_eq!(seen_left, expected_left);
        prop_assert_eq!(seen_right, expected_right);

        for window in rows.windows(2) {
            prop_assert_ne!(compare_rows(&window[0], &window[1]), Ordering::Greater);
        }
    }
}
